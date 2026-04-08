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
    ctx = _make_context("jetson_orin")
    assert ctx.topic("camera/front/compressed") == (
        "bubbaloop/global/jetson_orin/camera/front/compressed"
    )


def test_local_topic_formatting():
    ctx = _make_context("my_robot")
    assert ctx.local_topic("sensor/imu") == "bubbaloop/local/my_robot/sensor/imu"


def test_topic_wildcard_suffix():
    ctx = _make_context("edge_01")
    assert ctx.topic("**") == "bubbaloop/global/edge_01/**"


# ---------------------------------------------------------------------------
# _resolve_topic()
# ---------------------------------------------------------------------------

def test_resolve_topic_global():
    ctx = _make_context("bot")
    assert ctx._resolve_topic("data", False) == "bubbaloop/global/bot/data"


def test_resolve_topic_local():
    ctx = _make_context("bot")
    assert ctx._resolve_topic("raw", True) == "bubbaloop/local/bot/raw"


def test_global_and_local_share_suffix():
    ctx = _make_context("edge_42")
    global_topic = ctx.topic("sensor/data")
    local_topic = ctx.local_topic("sensor/data")
    assert global_topic.endswith("edge_42/sensor/data")
    assert local_topic.endswith("edge_42/sensor/data")
    assert "global" in global_topic
    assert "local" in local_topic


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
    from bubbaloop_sdk import ProtoSubscriber, RawSubscriber
    assert ProtoSubscriber is not None
    assert RawSubscriber is not None


def test_import_run_node():
    from bubbaloop_sdk import run_node
    assert callable(run_node)


# ---------------------------------------------------------------------------
# Shutdown
# ---------------------------------------------------------------------------

def test_shutdown_not_set_initially():
    ctx = _make_context("bot")
    assert not ctx.is_shutdown()


def test_shutdown_set_manually():
    ctx = _make_context("bot")
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
# RawPublisher.put()
# ---------------------------------------------------------------------------

def test_raw_publisher_puts_bytes():
    from bubbaloop_sdk.publisher import RawPublisher
    mock_pub = MagicMock()
    RawPublisher(mock_pub).put(b"\x00\x01\x02")
    mock_pub.put.assert_called_once_with(b"\x00\x01\x02")


def test_raw_publisher_converts_bytearray():
    from bubbaloop_sdk.publisher import RawPublisher
    mock_pub = MagicMock()
    RawPublisher(mock_pub).put(bytearray([0xFF, 0xFE]))
    mock_pub.put.assert_called_once_with(b"\xff\xfe")


# ---------------------------------------------------------------------------
# Import surface (new exports)
# ---------------------------------------------------------------------------

def test_import_raw_publisher():
    from bubbaloop_sdk import RawPublisher
    assert RawPublisher is not None


def test_import_proto_decoder():
    from bubbaloop_sdk import ProtoDecoder
    assert ProtoDecoder is not None


def test_import_discover_nodes():
    from bubbaloop_sdk import discover_nodes
    assert callable(discover_nodes)


def test_import_get_sample():
    from bubbaloop_sdk import get_sample, GetSampleTimeout
    assert callable(get_sample)
    assert GetSampleTimeout is not None


# ---------------------------------------------------------------------------
# Helper
# ---------------------------------------------------------------------------

def _make_context(machine_id: str):
    from bubbaloop_sdk.context import NodeContext
    ctx = object.__new__(NodeContext)
    ctx.session = MagicMock()
    ctx.machine_id = machine_id
    ctx.instance_name = machine_id
    ctx._shutdown = threading.Event()
    return ctx
