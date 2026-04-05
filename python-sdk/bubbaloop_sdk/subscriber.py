"""Zenoh subscribers — blocking and callback-based."""

from __future__ import annotations

import concurrent.futures
import queue
import threading

import zenoh

# Sentinel object pushed into a queue to unblock recv() on undeclare().
_CLOSED = object()


class TypedSubscriber:
    """Blocking subscriber with optional timeout. Iterates with ``for msg in sub``.

    Internally queue-backed: Zenoh delivers raw payload bytes via a callback into a
    ``queue.Queue``. Decoding (``msg_class.FromString``) happens in ``recv()`` on the
    consumer thread, not in Zenoh's internal thread::

        while not ctx.is_shutdown():
            msg = sub.recv(timeout=5.0)
            if msg is None:
                continue
            process(msg)

    Without a timeout, ``recv()`` blocks until a message arrives or ``undeclare()``
    is called (which pushes a sentinel to unblock any waiting ``recv()``).
    """

    def __init__(self, session: zenoh.Session, topic: str, msg_class=None):
        self._queue: queue.Queue = queue.Queue()
        self._msg_class = msg_class

        def _on_sample(sample: zenoh.Sample) -> None:
            self._queue.put(bytes(sample.payload.to_bytes()))

        self._sub = session.declare_subscriber(topic, _on_sample)

    def recv(self, timeout: float | None = None):
        """Block until the next message arrives. Returns ``None`` on timeout or close."""
        try:
            payload = self._queue.get(timeout=timeout)
        except queue.Empty:
            return None
        if payload is _CLOSED:
            return None
        if self._msg_class is not None and hasattr(self._msg_class, "FromString"):
            return self._msg_class.FromString(payload)
        return payload

    def __iter__(self):
        return self

    def __next__(self):
        msg = self.recv()
        if msg is None:
            raise StopIteration
        return msg

    def undeclare(self) -> None:
        """Undeclare the subscriber and unblock any waiting ``recv()``."""
        self._sub.undeclare()
        self._queue.put(_CLOSED)


class RawSubscriber:
    """Blocking subscriber that yields raw ``zenoh.Sample`` objects, with optional timeout.

    Internally queue-backed — same pattern as ``TypedSubscriber``.
    ``undeclare()`` pushes a sentinel to unblock any waiting ``recv()``.
    """

    def __init__(self, session: zenoh.Session, key_expr: str):
        self._queue: queue.Queue = queue.Queue()

        def _on_sample(sample: zenoh.Sample) -> None:
            self._queue.put(sample)

        self._sub = session.declare_subscriber(key_expr, _on_sample)

    def recv(self, timeout: float | None = None):
        """Block until the next sample arrives. Returns ``None`` on timeout or close."""
        try:
            sample = self._queue.get(timeout=timeout)
        except queue.Empty:
            return None
        if sample is _CLOSED:
            return None
        return sample

    def __iter__(self):
        return self

    def __next__(self):
        sample = self.recv()
        if sample is None:
            raise StopIteration
        return sample

    def undeclare(self) -> None:
        """Undeclare the subscriber and unblock any waiting ``recv()``."""
        self._sub.undeclare()
        self._queue.put(_CLOSED)


class CallbackSubscriber:
    """Callback-based subscriber — Zenoh calls ``handler`` from its internal thread.

    No loop required from the caller. Callbacks are invoked **serially** on Zenoh's
    single internal thread per session — if your handler is slow it will delay every
    other subscriber and queryable on the same session. Use ``subscriber_callback_async()``
    for slow work.

    ``handler`` receives a decoded message (``msg_class.FromString(payload)``) if
    ``msg_class`` is provided, or raw ``bytes`` otherwise.

    **Threading contract:** ``handler`` runs on Zenoh's internal thread.
    Protect shared state with a lock if accessed from other threads::

        lock = threading.Lock()
        last_value = None

        def on_msg(msg):
            nonlocal last_value
            with lock:
                last_value = msg

        sub = ctx.subscriber_callback("sensor/data", on_msg, SensorData)

    Keep the returned object alive — garbage-collecting it undeclares the subscriber.
    """

    def __init__(self, session: zenoh.Session, topic: str, handler, msg_class=None):
        def _wrap(sample: zenoh.Sample) -> None:
            payload = bytes(sample.payload.to_bytes())
            if msg_class is not None and hasattr(msg_class, "FromString"):
                handler(msg_class.FromString(payload))
            else:
                handler(payload)

        self._sub = session.declare_subscriber(topic, _wrap)

    def undeclare(self) -> None:
        """Undeclare the subscriber and stop receiving samples."""
        self._sub.undeclare()


class RawCallbackSubscriber:
    """Callback-based subscriber that passes raw ``zenoh.Sample`` to the handler.

    Use when you need access to the full sample metadata (key_expr, encoding,
    timestamp). Handler is called **serially** on Zenoh's internal thread.

    For slow handlers, use ``ctx.subscriber_raw_callback_async()`` instead.

    Keep the returned object alive — garbage-collecting it undeclares the subscriber.
    """

    def __init__(self, session: zenoh.Session, key_expr: str, handler):
        self._sub = session.declare_subscriber(key_expr, handler)

    def undeclare(self) -> None:
        """Undeclare the subscriber and stop receiving samples."""
        self._sub.undeclare()


class CallbackSubscriberAsync:
    """Callback subscriber that runs ``handler`` in a ``ThreadPoolExecutor``.

    **Use this when your handler does slow work** (database writes, hardware reads,
    HTTP calls). Zenoh uses a single internal thread for all callbacks — if a handler
    blocks, ALL other subscribers and queryables on the same session are delayed
    until it returns. ``CallbackSubscriberAsync`` fixes this by submitting the
    handler to a thread pool immediately and returning, freeing Zenoh's thread::

        # PROBLEM: on_insert blocks Zenoh's thread for 200ms per message
        sub = ctx.subscriber_callback("data", on_insert)

        # SOLUTION: handler runs in thread pool, Zenoh thread is free instantly
        sub = ctx.subscriber_callback_async("data", on_insert)

    **Threading contract:** multiple invocations of ``handler`` may run concurrently
    if messages arrive faster than the handler processes them. Protect shared state
    with locks.

    Keep the returned object alive — garbage-collecting it undeclares the subscriber.
    """

    def __init__(self, session: zenoh.Session, topic: str, handler, msg_class=None, max_workers: int = 4):
        self._executor = concurrent.futures.ThreadPoolExecutor(max_workers=max_workers)
        self._closing = threading.Event()

        def _wrap(sample: zenoh.Sample) -> None:
            if self._closing.is_set():
                return
            payload = bytes(sample.payload.to_bytes())

            def _decode_and_call():
                if msg_class is not None and hasattr(msg_class, "FromString"):
                    handler(msg_class.FromString(payload))
                else:
                    handler(payload)

            try:
                self._executor.submit(_decode_and_call)
            except RuntimeError:
                pass  # executor already shut down — drop the message

        self._sub = session.declare_subscriber(topic, _wrap)

    def undeclare(self) -> None:
        """Undeclare the subscriber and shutdown the thread pool."""
        self._closing.set()
        self._sub.undeclare()  # stop Zenoh callbacks first
        self._executor.shutdown(wait=False, cancel_futures=True)


class RawCallbackSubscriberAsync:
    """Raw callback subscriber that runs ``handler`` in a ``ThreadPoolExecutor``.

    Same as ``CallbackSubscriberAsync`` but passes raw ``zenoh.Sample`` objects.
    Use when you need sample metadata AND your handler does slow work.

    Keep the returned object alive — garbage-collecting it undeclares the subscriber.
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

    def undeclare(self) -> None:
        """Undeclare the subscriber and shutdown the thread pool."""
        self._closing.set()
        self._sub.undeclare()  # stop Zenoh callbacks first
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

    Keep the returned object alive — garbage-collecting it undeclares the queryable.
    """

    def __init__(self, session: zenoh.Session, key_expr: str, handler, max_workers: int = 4):
        self._executor = concurrent.futures.ThreadPoolExecutor(max_workers=max_workers)
        self._closing = threading.Event()

        def _wrap(query) -> None:
            if self._closing.is_set():
                return
            try:
                self._executor.submit(handler, query)
            except RuntimeError:
                pass  # executor already shut down — drop the query

        self._qbl = session.declare_queryable(key_expr, _wrap)

    def undeclare(self) -> None:
        """Undeclare the queryable and shutdown the thread pool."""
        self._closing.set()
        self._qbl.undeclare()  # stop Zenoh callbacks first
        self._executor.shutdown(wait=False, cancel_futures=True)
