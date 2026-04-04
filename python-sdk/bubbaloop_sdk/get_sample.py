"""Subscribe-and-wait helper for pulling one sample from a publishing node."""

import asyncio

import zenoh


class GetSampleTimeout(Exception):
    """Raised when no sample arrives within the deadline."""

    def __init__(self, topic: str, timeout: float):
        super().__init__(f"get_sample timed out after {timeout}s waiting on '{topic}'")
        self.topic = topic
        self.timeout = timeout


async def get_sample(
    session: zenoh.Session,
    key_expr: str,
    timeout: float = 5.0,
) -> zenoh.Sample:
    """Subscribe to *key_expr* and return the first sample received within *timeout* seconds.

    Useful for agents that need to pull a current value from a continuously-publishing
    node without maintaining a long-lived subscription. A new subscriber is declared,
    one sample is awaited, then it is undeclared.

    Raises:
        GetSampleTimeout: if no sample arrives before the deadline.

    Example::

        sample = await get_sample(session, "bubbaloop/local/host/openmeteo/weather", timeout=5)
        import json
        weather = json.loads(bytes(sample.payload))
    """
    loop = asyncio.get_event_loop()
    future: asyncio.Future[zenoh.Sample] = loop.create_future()

    def _handler(sample: zenoh.Sample) -> None:
        if not future.done():
            loop.call_soon_threadsafe(future.set_result, sample)

    sub = session.declare_subscriber(key_expr, _handler)
    try:
        return await asyncio.wait_for(asyncio.shield(future), timeout=timeout)
    except asyncio.TimeoutError:
        raise GetSampleTimeout(key_expr, timeout)
    finally:
        sub.undeclare()
