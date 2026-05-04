"""Tests for FakeContext."""

from __future__ import annotations

import threading
import time

from bubbaloop_sdk.testing import FakeContext, FakeZenohSession


def test_fake_context_defaults():
    ctx = FakeContext()
    assert isinstance(ctx.session, FakeZenohSession)
    assert ctx.machine_id == "test_machine"
    assert ctx.instance_name is None
    assert ctx.is_shutdown() is False


def test_fake_context_signal_shutdown_unblocks_wait():
    ctx = FakeContext()
    waiter = threading.Thread(target=ctx.wait_shutdown)
    waiter.start()
    assert waiter.is_alive()

    ctx.signal_shutdown()
    waiter.join(timeout=1.0)
    assert not waiter.is_alive()
    assert ctx.is_shutdown() is True


def test_fake_context_accepts_explicit_session():
    s = FakeZenohSession()
    ctx = FakeContext(session=s)
    assert ctx.session is s


def test_fake_context_propagates_machine_and_instance():
    ctx = FakeContext(machine_id="bot", instance_name="rec")
    assert ctx.machine_id == "bot"
    assert ctx.instance_name == "rec"


def test_wait_shutdown_returns_immediately_if_already_set():
    ctx = FakeContext()
    ctx.signal_shutdown()
    start = time.monotonic()
    ctx.wait_shutdown()
    assert time.monotonic() - start < 0.1
