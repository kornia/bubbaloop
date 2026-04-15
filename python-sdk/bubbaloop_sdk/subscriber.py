"""Zenoh subscribers — blocking and callback-based."""

from __future__ import annotations

import concurrent.futures
import threading

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


class CallbackSubscriber(_BaseSubscriber):
    """Callback-based subscriber — Zenoh calls ``handler`` from its internal thread.

    ``handler`` receives a decoded message: protobuf object, ``dict`` (JSON),
    or raw ``bytes`` — determined automatically by the sample's encoding header
    via the shared :class:`~bubbaloop_sdk.schema_registry.SchemaRegistry`.

    **Threading contract:** ``handler`` runs on Zenoh's internal thread.
    Protect shared state with a lock if accessed from other threads.

    Call ``undeclare()`` when done to stop receiving samples.
    """

    def __init__(self, session: zenoh.Session, topic: str, handler, registry):
        self._sub = session.declare_subscriber(topic, lambda sample: handler(registry.decode(sample)))
        self._undeclared = False


class RawCallbackSubscriber(_BaseSubscriber):
    """Callback-based subscriber that passes raw ``zenoh.Sample`` to the handler.

    Use when you need access to the full sample metadata (key_expr, encoding,
    timestamp). Handler is called **serially** on Zenoh's internal thread.

    For slow handlers, use ``ctx.subscriber_raw_callback_async()`` instead.

    Call ``undeclare()`` when done to stop receiving samples.
    """

    def __init__(self, session: zenoh.Session, key_expr: str, handler):
        self._sub = session.declare_subscriber(key_expr, handler)
        self._undeclared = False


class CallbackSubscriberAsync(_BaseSubscriber):
    """Callback subscriber that runs ``handler`` in a ``ThreadPoolExecutor``.

    ``handler`` receives auto-decoded messages (proto, dict, or bytes).
    Zenoh's internal thread is freed instantly; the handler runs in a thread pool.

    **Threading contract:** multiple invocations of ``handler`` may run concurrently.
    Protect shared state with locks.

    Call ``undeclare()`` when done to stop receiving samples.
    """

    def __init__(self, session: zenoh.Session, topic: str, handler, registry, max_workers: int = 4):
        self._executor = concurrent.futures.ThreadPoolExecutor(max_workers=max_workers)
        self._closing = threading.Event()

        def _wrap(sample: zenoh.Sample) -> None:
            if self._closing.is_set():
                return
            try:
                self._executor.submit(handler, registry.decode(sample))
            except RuntimeError:
                pass  # executor already shut down — drop the message

        self._sub = session.declare_subscriber(topic, _wrap)
        self._undeclared = False

    def undeclare(self) -> None:
        """Undeclare the subscriber and shutdown the thread pool. Idempotent."""
        if self._undeclared:
            return
        self._closing.set()
        super().undeclare()
        self._executor.shutdown(wait=False, cancel_futures=True)


class RawCallbackSubscriberAsync(_BaseSubscriber):
    """Raw callback subscriber that runs ``handler`` in a ``ThreadPoolExecutor``.

    Same as ``CallbackSubscriberAsync`` but passes raw ``zenoh.Sample`` objects.
    Use when you need sample metadata AND your handler does slow work.

    Call ``undeclare()`` when done to stop receiving samples.
    """

    def __init__(self, session: zenoh.Session, key_expr: str, handler, max_workers: int = 4):
        self._executor = concurrent.futures.ThreadPoolExecutor(max_workers=max_workers)
        self._closing = threading.Event()

        def _wrap(sample: zenoh.Sample) -> None:
            if self._closing.is_set():
                return
            try:
                self._executor.submit(handler, sample)
            except RuntimeError:
                pass  # executor already shut down — drop the message

        self._sub = session.declare_subscriber(key_expr, _wrap)
        self._undeclared = False

    def undeclare(self) -> None:
        """Undeclare the subscriber and shutdown the thread pool. Idempotent."""
        if self._undeclared:
            return
        self._closing.set()
        super().undeclare()
        self._executor.shutdown(wait=False, cancel_futures=True)


class AsyncQueryable:
    """Wrapper around ``zenoh.Queryable`` that runs the handler in a ``ThreadPoolExecutor``.

    **Use this (via ``ctx.queryable_async()``) when your queryable handler does slow work**
    (database reads, hardware access, network calls). Zenoh uses a single internal thread
    for all callbacks — a slow handler blocks ALL other subscribers and queryables on the
    same session. ``AsyncQueryable`` fixes this by submitting the handler to a thread pool
    immediately and returning, freeing Zenoh's thread::

        def on_db_query(query: zenoh.Query) -> None:
            rows = db.fetch(query.payload.to_string())   # slow
            query.reply(query.key_expr, json.dumps(rows).encode())

        qbl = ctx.queryable_async("device_data", on_db_query)
        # qbl.undeclare() when done — shuts down Zenoh queryable AND thread pool

    **Threading contract:** multiple invocations of ``handler`` may run concurrently
    if queries arrive faster than the handler processes them. Protect shared state
    with locks.

    Call ``undeclare()`` when done to stop receiving queries and release the thread pool.
    """

    def __init__(self, session: zenoh.Session, key_expr: str, handler, max_workers: int = 4):
        self._executor = concurrent.futures.ThreadPoolExecutor(max_workers=max_workers)
        self._closing = threading.Event()
        self._undeclared = False

        def _wrap(query) -> None:
            if self._closing.is_set():
                return
            try:
                self._executor.submit(handler, query)
            except RuntimeError:
                pass  # executor already shut down — drop the query

        self._qbl = session.declare_queryable(key_expr, _wrap)

    def undeclare(self) -> None:
        """Undeclare the queryable and shutdown the thread pool. Idempotent."""
        if self._undeclared:
            return
        self._undeclared = True
        self._closing.set()
        self._qbl.undeclare()  # stop Zenoh callbacks first
        self._executor.shutdown(wait=False, cancel_futures=True)
