"""FakeContext — drop-in stand-in for `NodeContext` in unit tests.

Mirrors the small subset of attributes/methods bubbaloop nodes actually
read off `ctx`: `session`, `machine_id`, `instance_name`, `wait_shutdown()`,
`is_shutdown()`. Backed by a `FakeZenohSession`.

A typical test runs the node in a thread, drives it via the session's
publish/query helpers, then trips shutdown so `run()` returns:

    ctx = FakeContext(machine_id="test", instance_name="rec")
    node = MyNode(ctx, {"name": "rec"})
    t = threading.Thread(target=node.run)
    t.start()
    ctx.session.query("bubbaloop/global/test/rec/command", b'{"command":"..."}')
    ctx.signal_shutdown()
    t.join(timeout=2)
"""

from __future__ import annotations

import threading

from .zenoh import FakeZenohSession


class FakeContext:
    """Stand-in for `bubbaloop_sdk.NodeContext` in unit tests."""

    def __init__(
        self,
        machine_id: str = "test_machine",
        instance_name: str | None = None,
        session: FakeZenohSession | None = None,
    ) -> None:
        self.session = session if session is not None else FakeZenohSession()
        self.machine_id = machine_id
        self.instance_name = instance_name
        self._shutdown = threading.Event()

    def is_shutdown(self) -> bool:
        return self._shutdown.is_set()

    def wait_shutdown(self) -> None:
        """Block until `signal_shutdown()` is called. Production
        `NodeContext.wait_shutdown()` blocks on SIGINT/SIGTERM; tests
        trip it explicitly."""
        self._shutdown.wait()

    def signal_shutdown(self) -> None:
        """Test-side: trip the shutdown event so a node's `run()` returns."""
        self._shutdown.set()
