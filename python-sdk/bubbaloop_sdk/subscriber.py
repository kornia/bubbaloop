"""Zenoh subscribers — blocking and callback-based."""

import queue

import zenoh


class TypedSubscriber:
    """Blocking subscriber with optional timeout. Iterates with ``for msg in sub``.

    Internally queue-backed: Zenoh delivers samples via a callback into a
    ``queue.Queue``. ``recv()`` drains the queue with an optional timeout,
    allowing clean shutdown integration::

        while not ctx.is_shutdown():
            msg = sub.recv(timeout=5.0)
            if msg is None:
                continue
            process(msg)

    Without a timeout, ``recv()`` blocks indefinitely (backward-compatible).
    """

    def __init__(self, session: zenoh.Session, topic: str, msg_class=None):
        self._queue: queue.Queue = queue.Queue()
        self._msg_class = msg_class

        def _on_sample(sample: zenoh.Sample) -> None:
            payload = bytes(sample.payload.to_bytes())
            if self._msg_class is not None and hasattr(self._msg_class, "FromString"):
                self._queue.put(self._msg_class.FromString(payload))
            else:
                self._queue.put(payload)

        self._sub = session.declare_subscriber(topic, _on_sample)

    def recv(self, timeout: float | None = None):
        """Block until the next message arrives. Returns ``None`` on timeout."""
        try:
            return self._queue.get(timeout=timeout)
        except queue.Empty:
            return None

    def __iter__(self):
        return self

    def __next__(self):
        msg = self.recv()
        if msg is None:
            raise StopIteration
        return msg

    def undeclare(self) -> None:
        """Undeclare the subscriber and stop receiving samples."""
        self._sub.undeclare()


class RawSubscriber:
    """Blocking subscriber that yields raw ``zenoh.Sample`` objects, with optional timeout.

    Internally queue-backed — same pattern as ``TypedSubscriber``.
    ``recv()`` also exposes an optional timeout for shutdown-aware loops.
    """

    def __init__(self, session: zenoh.Session, key_expr: str):
        self._queue: queue.Queue = queue.Queue()

        def _on_sample(sample: zenoh.Sample) -> None:
            self._queue.put(sample)

        self._sub = session.declare_subscriber(key_expr, _on_sample)

    def recv(self, timeout: float | None = None):
        """Block until the next sample arrives. Returns ``None`` on timeout."""
        try:
            return self._queue.get(timeout=timeout)
        except queue.Empty:
            return None

    def __iter__(self):
        return self

    def __next__(self):
        sample = self.recv()
        if sample is None:
            raise StopIteration
        return sample

    def undeclare(self) -> None:
        """Undeclare the subscriber and stop receiving samples."""
        self._sub.undeclare()


class CallbackSubscriber:
    """Callback-based subscriber — Zenoh calls ``handler`` from its internal thread.

    No loop required from the caller. All declared ``CallbackSubscriber`` instances
    on the same session receive concurrently and independently.

    ``handler`` receives a decoded message (``msg_class.FromString(payload)``) if
    ``msg_class`` is provided, or raw ``bytes`` otherwise.

    **Threading contract:** ``handler`` is called from Zenoh's internal thread.
    If you share state with a main loop, protect it with a lock::

        lock = threading.Lock()
        last_value = None

        def on_msg(msg):
            nonlocal last_value
            with lock:
                last_value = msg

        sub = ctx.subscriber_callback("sensor/data", on_msg, SensorData)

    For slow handlers (database writes, hardware I/O), use
    ``ctx.subscriber_callback_async()`` instead to avoid blocking Zenoh's thread.

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
    timestamp). Handler is called from Zenoh's internal thread.

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
        import concurrent.futures
        self._executor = concurrent.futures.ThreadPoolExecutor(max_workers=max_workers)

        def _wrap(sample: zenoh.Sample) -> None:
            payload = bytes(sample.payload.to_bytes())
            if msg_class is not None and hasattr(msg_class, "FromString"):
                msg = msg_class.FromString(payload)
            else:
                msg = payload
            self._executor.submit(handler, msg)

        self._sub = session.declare_subscriber(topic, _wrap)

    def undeclare(self) -> None:
        """Shutdown the thread pool and undeclare the subscriber."""
        self._executor.shutdown(wait=False)
        self._sub.undeclare()


class RawCallbackSubscriberAsync:
    """Raw callback subscriber that runs ``handler`` in a ``ThreadPoolExecutor``.

    Same as ``CallbackSubscriberAsync`` but passes raw ``zenoh.Sample`` objects.
    Use when you need sample metadata AND your handler does slow work.

    Keep the returned object alive — garbage-collecting it undeclares the subscriber.
    """

    def __init__(self, session: zenoh.Session, key_expr: str, handler, max_workers: int = 4):
        import concurrent.futures
        self._executor = concurrent.futures.ThreadPoolExecutor(max_workers=max_workers)

        def _wrap(sample: zenoh.Sample) -> None:
            self._executor.submit(handler, sample)

        self._sub = session.declare_subscriber(key_expr, _wrap)

    def undeclare(self) -> None:
        """Shutdown the thread pool and undeclare the subscriber."""
        self._executor.shutdown(wait=False)
        self._sub.undeclare()
