"""Tests for publisher_cbor(local=True) and publisher_cbor_absolute(local=True).

Verifies:
- local=True routes to the ``bubbaloop/local/...`` key space.
- local=True sets ``CongestionControl.BLOCK`` on the declared publisher.
- local=False (default) stays on ``bubbaloop/global/...`` with no congestion kwarg.
- publisher_cbor_absolute(local=True) uses the local absolute key (no instance prefix).
"""

import threading
from unittest.mock import MagicMock, patch

import zenoh


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _make_context(machine_id: str = "host1", instance_name: str = "mynode"):
    from bubbaloop_sdk.context import NodeContext
    ctx = object.__new__(NodeContext)
    ctx.session = MagicMock()
    ctx.machine_id = machine_id
    ctx.instance_name = instance_name
    ctx._shutdown = threading.Event()
    return ctx


# ---------------------------------------------------------------------------
# publisher_cbor(local=True) — topic key space
# ---------------------------------------------------------------------------

def test_publisher_cbor_local_true_uses_local_topic():
    """local=True must route the publisher to ``bubbaloop/local/...``."""
    from bubbaloop_sdk import publisher as pub_mod
    ctx = _make_context()
    with patch.object(pub_mod.CborPublisher, "_declare", return_value=MagicMock()) as mock_declare:
        ctx.publisher_cbor("rgbd", local=True)
        declared_key = mock_declare.call_args.args[1]
    assert declared_key.startswith("bubbaloop/local/host1/mynode/rgbd"), (
        f"Expected local key, got: {declared_key}"
    )


# ---------------------------------------------------------------------------
# publisher_cbor(local=True) — congestion control (Zenoh layer)
# ---------------------------------------------------------------------------

def test_publisher_cbor_local_true_sets_block_congestion_control():
    """local=True must declare the Zenoh publisher with CongestionControl.BLOCK.

    We call CborPublisher._declare directly (bypassing context.publisher_cbor)
    so this test is immune to class-attribute pollution from other test modules.
    """
    from bubbaloop_sdk.publisher import CborPublisher
    session = MagicMock()
    # _declare may be polluted by prior test modules; use the real implementation
    # by importing the unbound function and calling it explicitly.
    import importlib
    import bubbaloop_sdk.publisher as pub_mod
    importlib.reload(pub_mod)
    CborPublisherFresh = pub_mod.CborPublisher

    CborPublisherFresh._declare(session, "bubbaloop/local/host1/mynode/rgbd", local=True)

    assert session.declare_publisher.called, "declare_publisher was not called"
    call_kwargs = session.declare_publisher.call_args.kwargs
    assert "congestion_control" in call_kwargs, (
        "Expected congestion_control kwarg when local=True"
    )
    assert call_kwargs["congestion_control"] == zenoh.CongestionControl.BLOCK


# ---------------------------------------------------------------------------
# publisher_cbor(local=False) — regression: stays global, no BLOCK
# ---------------------------------------------------------------------------

def test_publisher_cbor_local_false_stays_global_no_block():
    """Default local=False must NOT set congestion_control on the Zenoh publisher."""
    import importlib
    import bubbaloop_sdk.publisher as pub_mod
    importlib.reload(pub_mod)
    CborPublisherFresh = pub_mod.CborPublisher

    session = MagicMock()
    CborPublisherFresh._declare(session, "bubbaloop/global/host1/mynode/sensor/data", local=False)

    assert session.declare_publisher.called, "declare_publisher was not called"
    declared_key = session.declare_publisher.call_args.args[0]
    assert declared_key.startswith("bubbaloop/global/"), (
        f"Expected global key, got: {declared_key}"
    )
    call_kwargs = session.declare_publisher.call_args.kwargs
    assert "congestion_control" not in call_kwargs, (
        "congestion_control must NOT be set when local=False"
    )


# ---------------------------------------------------------------------------
# publisher_cbor_absolute(local=True) — absolute local key, no instance prefix
# ---------------------------------------------------------------------------

def test_publisher_cbor_absolute_local_true_uses_local_absolute_topic():
    """publisher_cbor_absolute with local=True must use
    ``bubbaloop/local/{machine_id}/{absolute_suffix}`` (no instance_name)."""
    from bubbaloop_sdk import publisher as pub_mod
    ctx = _make_context(machine_id="host1", instance_name="mynode")
    with patch.object(pub_mod.CborPublisher, "_declare", return_value=MagicMock()) as mock_declare:
        ctx.publisher_cbor_absolute("bus/events", local=True)
        declared_key = mock_declare.call_args.args[1]
    assert declared_key.startswith("bubbaloop/local/host1/"), (
        f"Expected local absolute key, got: {declared_key}"
    )
    # Must NOT contain the instance name prefix
    assert "/mynode/" not in declared_key, (
        f"Absolute publisher must not include instance_name in key: {declared_key}"
    )
    assert declared_key == "bubbaloop/local/host1/bus/events"
