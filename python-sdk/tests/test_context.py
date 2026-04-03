"""Unit tests for bubbaloop_sdk.

Tests do NOT open a real Zenoh session — only pure-Python logic.
"""

import json
import socket
import threading
from unittest.mock import MagicMock

import pytest


# ---------------------------------------------------------------------------
# topic()
# ---------------------------------------------------------------------------

def test_topic_formatting():
    ctx = _make_context("staging", "jetson_orin")
    assert ctx.topic("camera/front/compressed") == (
        "bubbaloop/staging/jetson_orin/camera/front/compressed"
    )


def test_topic_default_scope():
    ctx = _make_context("local", "my_robot")
    assert ctx.topic("sensor/imu") == "bubbaloop/local/my_robot/sensor/imu"


def test_topic_wildcard_suffix():
    ctx = _make_context("prod", "edge_01")
    assert ctx.topic("**") == "bubbaloop/prod/edge_01/**"


# ---------------------------------------------------------------------------
# _hostname() sanitization
# ---------------------------------------------------------------------------

def test_hostname_sanitization_hyphens(monkeypatch):
    from bubbaloop_sdk.context import _hostname
    monkeypatch.setattr(socket, "gethostname", lambda: "my-robot-01")
    assert _hostname() == "my_robot_01"


def test_hostname_no_hyphens(monkeypatch):
    from bubbaloop_sdk.context import _hostname
    monkeypatch.setattr(socket, "gethostname", lambda: "myrobot")
    assert _hostname() == "myrobot"


# ---------------------------------------------------------------------------
# Import surface
# ---------------------------------------------------------------------------

def test_import_node_context():
    from bubbaloop_sdk import NodeContext
    assert NodeContext is not None


def test_import_publishers():
    from bubbaloop_sdk import ProtoPublisher, JsonPublisher
    assert ProtoPublisher is not None
    assert JsonPublisher is not None


def test_import_subscribers():
    from bubbaloop_sdk import TypedSubscriber, RawSubscriber
    assert TypedSubscriber is not None
    assert RawSubscriber is not None


def test_import_run_node():
    from bubbaloop_sdk import run_node
    assert callable(run_node)


# ---------------------------------------------------------------------------
# Shutdown
# ---------------------------------------------------------------------------

def test_shutdown_not_set_initially():
    ctx = _make_context("local", "bot")
    assert not ctx.is_shutdown()


def test_shutdown_set_manually():
    ctx = _make_context("local", "bot")
    ctx._shutdown.set()
    assert ctx.is_shutdown()


# ---------------------------------------------------------------------------
# ProtoPublisher.put()
# ---------------------------------------------------------------------------

def test_proto_publisher_rejects_invalid_type():
    from bubbaloop_sdk.publisher import ProtoPublisher
    pub = ProtoPublisher(MagicMock(), None)
    with pytest.raises(TypeError):
        pub.put(12345)


def test_proto_publisher_accepts_bytes():
    from bubbaloop_sdk.publisher import ProtoPublisher
    mock_pub = MagicMock()
    ProtoPublisher(mock_pub, None).put(b"\x01\x02\x03")
    mock_pub.put.assert_called_once_with(b"\x01\x02\x03")


def test_proto_publisher_calls_serialize():
    from bubbaloop_sdk.publisher import ProtoPublisher
    fake_msg = MagicMock()
    fake_msg.SerializeToString.return_value = b"\xde\xad\xbe\xef"
    mock_pub = MagicMock()
    ProtoPublisher(mock_pub, "some.Type").put(fake_msg)
    mock_pub.put.assert_called_once_with(b"\xde\xad\xbe\xef")


# ---------------------------------------------------------------------------
# JsonPublisher.put()
# ---------------------------------------------------------------------------

def test_json_publisher_serializes_dict():
    from bubbaloop_sdk.publisher import JsonPublisher
    mock_pub = MagicMock()
    JsonPublisher(mock_pub).put({"temperature": 22.5})
    assert json.loads(mock_pub.put.call_args[0][0]) == {"temperature": 22.5}


def test_json_publisher_passthrough_bytes():
    from bubbaloop_sdk.publisher import JsonPublisher
    mock_pub = MagicMock()
    JsonPublisher(mock_pub).put(b"raw")
    mock_pub.put.assert_called_once_with(b"raw")


def test_json_publisher_passthrough_str():
    from bubbaloop_sdk.publisher import JsonPublisher
    mock_pub = MagicMock()
    JsonPublisher(mock_pub).put("hello")
    mock_pub.put.assert_called_once_with(b"hello")


# ---------------------------------------------------------------------------
# Helper
# ---------------------------------------------------------------------------

def _make_context(scope: str, machine_id: str):
    from bubbaloop_sdk.context import NodeContext
    ctx = object.__new__(NodeContext)
    ctx.session = MagicMock()
    ctx.scope = scope
    ctx.machine_id = machine_id
    ctx.instance_name = machine_id
    ctx._shutdown = threading.Event()
    return ctx
