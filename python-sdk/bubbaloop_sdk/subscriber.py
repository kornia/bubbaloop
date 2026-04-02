"""Typed and raw Zenoh subscribers.

zenoh-python's ``Subscriber.recv()`` is a blocking call.  Both subscriber
classes bridge this to asyncio by running ``recv()`` in a thread executor,
so ``async for msg in subscriber`` works without blocking the event loop.
"""

import asyncio

import zenoh


class TypedSubscriber:
    """Async-iterable subscriber that auto-decodes protobuf messages.

    If ``msg_class`` is provided (a generated protobuf class), each received
    payload is decoded via ``msg_class.FromString(payload)``.  Otherwise the
    raw ``bytes`` payload is yielded.

    Do not construct directly — use ``NodeContext.subscriber()``.

    Example::

        async for msg in await ctx.subscriber("weather/+/current", CurrentWeather):
            print(msg.temperature)
    """

    def __init__(
        self,
        session: zenoh.Session,
        topic: str,
        msg_class=None,
    ):
        self._subscriber: zenoh.Subscriber = session.declare_subscriber(topic)
        self._msg_class = msg_class
        self._loop = asyncio.get_event_loop()

    def __aiter__(self):
        return self

    async def __anext__(self):
        """Block (in executor) until the next sample arrives, then decode it."""
        try:
            sample = await self._loop.run_in_executor(
                None, self._subscriber.recv
            )
            payload = bytes(sample.payload.to_bytes())
            if self._msg_class is not None and hasattr(
                self._msg_class, "FromString"
            ):
                return self._msg_class.FromString(payload)
            return payload
        except Exception as exc:
            raise StopAsyncIteration from exc

    def undeclare(self) -> None:
        """Undeclare the underlying Zenoh subscriber."""
        self._subscriber.undeclare()


class RawSubscriber:
    """Async-iterable subscriber that exposes zenoh ``Sample`` objects directly.

    Use this when you need access to ``sample.encoding``, ``sample.key_expr``,
    or other metadata alongside the payload — for example when building a
    dashboard-style dynamic decoder.

    The key expression is used verbatim (no ``topic()`` prefix is applied).

    Do not construct directly — use ``NodeContext.subscriber_raw()``.

    Example::

        sub = await ctx.subscriber_raw("bubbaloop/**")
        async for sample in sub:
            enc = str(sample.encoding)
            payload = bytes(sample.payload.to_bytes())
    """

    def __init__(self, session: zenoh.Session, key_expr: str):
        self._subscriber: zenoh.Subscriber = session.declare_subscriber(key_expr)
        self._loop = asyncio.get_event_loop()

    def recv(self):
        """Blocking receive — returns a zenoh Sample directly."""
        return self._subscriber.recv()

    def __aiter__(self):
        return self

    async def __anext__(self):
        """Block (in executor) until the next sample arrives."""
        try:
            return await self._loop.run_in_executor(
                None, self._subscriber.recv
            )
        except Exception as exc:
            raise StopAsyncIteration from exc

    def undeclare(self) -> None:
        """Undeclare the underlying Zenoh subscriber."""
        self._subscriber.undeclare()
