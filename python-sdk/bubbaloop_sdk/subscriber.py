"""Zenoh subscribers — blocking and callback-based."""

from __future__ import annotations

import concurrent.futures
import threading
from collections.abc import Callable
from typing import TYPE_CHECKING, Any

import zenoh

if TYPE_CHECKING:
    from .schema_registry import SchemaRegistry


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

    def __init__(self, session: zenoh.Session, topic: str, registry: SchemaRegistry):
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


class CallbackSubscriber(_BaseSubscriber):
    """Callback-based subscriber with auto-decode via SchemaRegistry.

    ``handler`` receives a decoded message: protobuf object, ``dict`` (JSON),
    or raw ``bytes`` — determined automatically by the sample's encoding header.

    By default the handler runs on Zenoh's internal thread (fast path). Pass
    ``max_workers`` to run the handler in a ``ThreadPoolExecutor`` instead —
    use this when the handler does slow work (DB writes, HTTP calls, hardware I/O).

    Args:
        session: Active Zenoh session.
        topic: Key expression to subscribe to.
        handler: Callable invoked with each decoded message.
        registry: SchemaRegistry for auto-decoding samples by encoding header.
        max_workers: If None (default), handler runs on Zenoh's thread. If int,
            handler runs in a ThreadPoolExecutor with that many threads.

    Call ``undeclare()`` when done to stop receiving samples.
    """

    def __init__(
        self,
        session: zenoh.Session,
        topic: str,
        handler: Callable[[Any], None],
        registry: SchemaRegistry,
        max_workers: int | None = None,
    ):
        self._executor: concurrent.futures.ThreadPoolExecutor | None = None
        self._closing: threading.Event | None = None

        if max_workers is not None:
            self._executor = concurrent.futures.ThreadPoolExecutor(max_workers=max_workers)
            self._closing = threading.Event()

            def _wrap(sample: zenoh.Sample) -> None:
                if self._closing.is_set():  # type: ignore[union-attr]
                    return
                try:
                    self._executor.submit(handler, registry.decode(sample))  # type: ignore[union-attr]
                except RuntimeError:
                    pass  # executor already shut down — drop the message

            self._sub = session.declare_subscriber(topic, _wrap)
        else:
            self._sub = session.declare_subscriber(topic, lambda sample: handler(registry.decode(sample)))
        self._undeclared = False

    def undeclare(self) -> None:
        """Undeclare the subscriber and shutdown the thread pool (if any). Idempotent."""
        if self._undeclared:
            return
        if self._closing is not None:
            self._closing.set()
        super().undeclare()
        if self._executor is not None:
            self._executor.shutdown(wait=False, cancel_futures=True)


class RawCallbackSubscriber(_BaseSubscriber):
    """Callback-based subscriber that passes raw ``zenoh.Sample`` to the handler.

    Use when you need access to the full sample metadata (key_expr, encoding,
    timestamp).

    By default the handler runs on Zenoh's internal thread. Pass ``max_workers``
    to run the handler in a ``ThreadPoolExecutor`` instead.

    Args:
        session: Active Zenoh session.
        key_expr: Literal key expression to subscribe to.
        handler: Callable invoked with each ``zenoh.Sample``.
        max_workers: If None (default), handler runs on Zenoh's thread. If int,
            handler runs in a ThreadPoolExecutor with that many threads.

    Call ``undeclare()`` when done to stop receiving samples.
    """

    def __init__(
        self,
        session: zenoh.Session,
        key_expr: str,
        handler: Callable[[zenoh.Sample], None],
        max_workers: int | None = None,
    ):
        self._executor: concurrent.futures.ThreadPoolExecutor | None = None
        self._closing: threading.Event | None = None

        if max_workers is not None:
            self._executor = concurrent.futures.ThreadPoolExecutor(max_workers=max_workers)
            self._closing = threading.Event()

            def _wrap(sample: zenoh.Sample) -> None:
                if self._closing.is_set():  # type: ignore[union-attr]
                    return
                try:
                    self._executor.submit(handler, sample)  # type: ignore[union-attr]
                except RuntimeError:
                    pass  # executor already shut down — drop the message

            self._sub = session.declare_subscriber(key_expr, _wrap)
        else:
            self._sub = session.declare_subscriber(key_expr, handler)
        self._undeclared = False

    def undeclare(self) -> None:
        """Undeclare the subscriber and shutdown the thread pool (if any). Idempotent."""
        if self._undeclared:
            return
        if self._closing is not None:
            self._closing.set()
        super().undeclare()
        if self._executor is not None:
            self._executor.shutdown(wait=False, cancel_futures=True)


class Queryable:
    """Queryable that responds to Zenoh GET requests.

    ``handler`` receives a ``zenoh.Query`` and must call ``query.reply()`` to respond.

    By default the handler runs on Zenoh's internal thread (fast path). Pass
    ``max_workers`` to run the handler in a ``ThreadPoolExecutor`` instead —
    use this when the handler does slow work (DB reads, hardware access, HTTP calls).

    **Important:** do NOT pass ``complete=True`` to the underlying queryable —
    it blocks wildcard queries like ``bubbaloop/**/schema`` used by the dashboard.

    Args:
        session: Active Zenoh session.
        key_expr: Key expression to declare the queryable on.
        handler: Callable invoked with each ``zenoh.Query``.
        max_workers: If None (default), handler runs on Zenoh's thread. If int,
            handler runs in a ThreadPoolExecutor with that many threads.

    Call ``undeclare()`` when done to stop receiving queries.
    """

    def __init__(
        self,
        session: zenoh.Session,
        key_expr: str,
        handler: Callable[[zenoh.Query], None],
        max_workers: int | None = None,
    ):
        self._executor: concurrent.futures.ThreadPoolExecutor | None = None
        self._closing: threading.Event | None = None
        self._undeclared = False

        if max_workers is not None:
            self._executor = concurrent.futures.ThreadPoolExecutor(max_workers=max_workers)
            self._closing = threading.Event()

            def _wrap(query: zenoh.Query) -> None:
                if self._closing.is_set():  # type: ignore[union-attr]
                    return
                try:
                    self._executor.submit(handler, query)  # type: ignore[union-attr]
                except RuntimeError:
                    pass  # executor already shut down — drop the query

            self._qbl = session.declare_queryable(key_expr, _wrap)
        else:
            self._qbl = session.declare_queryable(key_expr, handler)

    def undeclare(self) -> None:
        """Undeclare the queryable and shutdown the thread pool (if any). Idempotent."""
        if self._undeclared:
            return
        self._undeclared = True
        if self._closing is not None:
            self._closing.set()
        self._qbl.undeclare()
        if self._executor is not None:
            self._executor.shutdown(wait=False, cancel_futures=True)
