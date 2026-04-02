"""Unit tests for bubbaloop_sdk.

These tests do NOT open a real Zenoh session — they only test pure-Python
logic that can run without a zenoh router.
"""

import sys
import os

import pytest


# ---------------------------------------------------------------------------
# topic() formatting
# ---------------------------------------------------------------------------


def test_topic_formatting():
    """NodeContext.topic() must produce the canonical bubbaloop topic pattern."""
    # Import only what we need without triggering zenoh.open()
    from bubbaloop_sdk.context import NodeContext

    # Build a minimal context without a real session
    ctx = _make_context("staging", "jetson_orin")
    assert ctx.topic("camera/front/compressed") == (
        "bubbaloop/staging/jetson_orin/camera/front/compressed"
    )


def test_topic_default_scope():
    from bubbaloop_sdk.context import NodeContext

    ctx = _make_context("local", "my_robot")
    assert ctx.topic("sensor/imu") == "bubbaloop/local/my_robot/sensor/imu"


def test_topic_wildcard_suffix():
    from bubbaloop_sdk.context import NodeContext

    ctx = _make_context("prod", "edge_01")
    # Wildcards are valid in Zenoh key expressions
    assert ctx.topic("**") == "bubbaloop/prod/edge_01/**"


# ---------------------------------------------------------------------------
# _get_hostname() sanitization
# ---------------------------------------------------------------------------


def test_hostname_sanitization_hyphens():
    """_get_hostname() replaces hyphens with underscores."""
    from bubbaloop_sdk.context import _get_hostname

    # Monkeypatch socket.gethostname to return a hyphenated hostname
    import socket
    original = socket.gethostname

    try:
        socket.gethostname = lambda: "my-robot-01"
        result = _get_hostname()
        assert result == "my_robot_01"
    finally:
        socket.gethostname = original


def test_hostname_sanitization_no_hyphens():
    from bubbaloop_sdk.context import _get_hostname

    import socket
    original = socket.gethostname

    try:
        socket.gethostname = lambda: "myrobot"
        assert _get_hostname() == "myrobot"
    finally:
        socket.gethostname = original


# ---------------------------------------------------------------------------
# Import surface — verify all public classes are importable
# ---------------------------------------------------------------------------


def test_import_node_context():
    from bubbaloop_sdk import NodeContext
    assert NodeContext is not None


def test_import_proto_publisher():
    from bubbaloop_sdk import ProtoPublisher
    assert ProtoPublisher is not None


def test_import_json_publisher():
    from bubbaloop_sdk import JsonPublisher
    assert JsonPublisher is not None


def test_import_typed_subscriber():
    from bubbaloop_sdk import TypedSubscriber
    assert TypedSubscriber is not None


def test_import_raw_subscriber():
    from bubbaloop_sdk import RawSubscriber
    assert RawSubscriber is not None


def test_import_schema_helper():
    from bubbaloop_sdk.schema import declare_schema_queryable
    assert callable(declare_schema_queryable)


# ---------------------------------------------------------------------------
# shutdown event logic
# ---------------------------------------------------------------------------


def test_shutdown_not_set_initially():
    ctx = _make_context("local", "bot")
    assert not ctx.is_shutdown()


def test_shutdown_set_manually():
    ctx = _make_context("local", "bot")
    ctx._shutdown_event.set()
    assert ctx.is_shutdown()


# ---------------------------------------------------------------------------
# ProtoPublisher.put() input validation
# ---------------------------------------------------------------------------


def test_proto_publisher_rejects_invalid_type():
    """put() should raise TypeError for non-protobuf, non-bytes inputs."""
    import asyncio
    from unittest.mock import MagicMock
    from bubbaloop_sdk.publisher import ProtoPublisher

    mock_pub = MagicMock()
    publisher = ProtoPublisher(mock_pub, "some.TypeName")

    with pytest.raises(TypeError):
        asyncio.get_event_loop().run_until_complete(publisher.put(12345))


def test_proto_publisher_accepts_bytes():
    import asyncio
    from unittest.mock import MagicMock
    from bubbaloop_sdk.publisher import ProtoPublisher

    mock_pub = MagicMock()
    publisher = ProtoPublisher(mock_pub, None)
    asyncio.get_event_loop().run_until_complete(publisher.put(b"\x01\x02\x03"))
    mock_pub.put.assert_called_once_with(b"\x01\x02\x03")


def test_proto_publisher_accepts_proto_message():
    """put() calls SerializeToString() on protobuf-like objects."""
    import asyncio
    from unittest.mock import MagicMock
    from bubbaloop_sdk.publisher import ProtoPublisher

    # Simulate a protobuf message
    fake_msg = MagicMock()
    fake_msg.SerializeToString.return_value = b"\xde\xad\xbe\xef"

    mock_pub = MagicMock()
    publisher = ProtoPublisher(mock_pub, "some.TypeName")
    asyncio.get_event_loop().run_until_complete(publisher.put(fake_msg))
    fake_msg.SerializeToString.assert_called_once()
    mock_pub.put.assert_called_once_with(b"\xde\xad\xbe\xef")


# ---------------------------------------------------------------------------
# JsonPublisher.put() serialization
# ---------------------------------------------------------------------------


def test_json_publisher_serializes_dict():
    import asyncio
    import json
    from unittest.mock import MagicMock
    from bubbaloop_sdk.publisher import JsonPublisher

    mock_pub = MagicMock()
    publisher = JsonPublisher(mock_pub)
    asyncio.get_event_loop().run_until_complete(
        publisher.put({"temperature": 22.5})
    )
    call_args = mock_pub.put.call_args[0][0]
    assert json.loads(call_args) == {"temperature": 22.5}


def test_json_publisher_passthrough_bytes():
    import asyncio
    from unittest.mock import MagicMock
    from bubbaloop_sdk.publisher import JsonPublisher

    mock_pub = MagicMock()
    publisher = JsonPublisher(mock_pub)
    asyncio.get_event_loop().run_until_complete(publisher.put(b"raw"))
    mock_pub.put.assert_called_once_with(b"raw")


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_context(scope: str, machine_id: str):
    """Build a NodeContext without opening a real Zenoh session."""
    from unittest.mock import MagicMock
    from bubbaloop_sdk.context import NodeContext

    ctx = object.__new__(NodeContext)
    ctx.session = MagicMock()
    ctx.scope = scope
    ctx.machine_id = machine_id

    import asyncio
    ctx._shutdown_event = asyncio.Event()
    return ctx
