"""Unit tests for bubbaloop_sdk.

Tests do NOT open a real Zenoh session — only pure-Python logic.
"""

import json
import socket
import threading
from unittest.mock import MagicMock

import cbor2
import pytest


# ---------------------------------------------------------------------------
# topic()
# ---------------------------------------------------------------------------

def test_topic_formatting_auto_scoped():
    ctx = _make_context("jetson_orin", instance_name="cam_embedder")
    assert ctx.topic("embeddings") == (
        "bubbaloop/global/jetson_orin/cam_embedder/embeddings"
    )


def test_local_topic_formatting_auto_scoped():
    ctx = _make_context("my_robot", instance_name="lidar_a")
    assert ctx.local_topic("raw") == "bubbaloop/local/my_robot/lidar_a/raw"


def test_topic_wildcard_suffix_auto_scoped():
    ctx = _make_context("edge_01", instance_name="node_x")
    assert ctx.topic("**") == "bubbaloop/global/edge_01/node_x/**"


def test_topic_falls_back_when_instance_name_unset():
    """When instance_name=None (NodeContext.connect() outside run_node()),
    topics fall back to the legacy ``bubbaloop/{scope}/{machine_id}/{suffix}`` layout."""
    ctx = _make_context("bot", instance_name=None)
    assert ctx.topic("data") == "bubbaloop/global/bot/data"
    assert ctx.local_topic("raw") == "bubbaloop/local/bot/raw"


# ---------------------------------------------------------------------------
# Absolute (un-scoped) topic helpers
# ---------------------------------------------------------------------------

def test_absolute_topic_skips_instance_name():
    ctx = _make_context("bot", instance_name="some_node")
    assert ctx.absolute_topic("bus/events") == "bubbaloop/global/bot/bus/events"
    assert ctx.absolute_local_topic("frames") == "bubbaloop/local/bot/frames"


def test_publisher_json_absolute_uses_unscoped_key():
    from bubbaloop_sdk.publisher import JsonPublisher
    ctx = _make_context("bot", instance_name="emb")
    fake = MagicMock()
    JsonPublisher._declare = MagicMock(return_value=fake)
    ctx.publisher_json_absolute("daemon/cmd")
    JsonPublisher._declare.assert_called_once_with(
        ctx.session,
        "bubbaloop/global/bot/daemon/cmd",
        source_instance="emb",
        schema_uri="bubbaloop://emb/daemon/cmd@v1",
    )


def test_publisher_cbor_absolute_uses_unscoped_key():
    from bubbaloop_sdk.publisher import CborPublisher
    ctx = _make_context("bot", instance_name="emb")
    fake = MagicMock()
    CborPublisher._declare = MagicMock(return_value=fake)
    ctx.publisher_cbor_absolute("bus/x", schema_uri="bubbaloop://bus/v1")
    CborPublisher._declare.assert_called_once_with(
        ctx.session,
        "bubbaloop/global/bot/bus/x",
        source_instance="emb",
        schema_uri="bubbaloop://bus/v1",
    )


def test_publisher_cbor_default_schema_uri_from_ctx():
    """When the caller doesn't supply schema_uri, the SDK synthesizes
    ``bubbaloop://{instance}/{topic_suffix}@v{schema_version}``."""
    from bubbaloop_sdk.publisher import CborPublisher
    ctx = _make_context("bot", instance_name="emb")
    fake = MagicMock()
    CborPublisher._declare = MagicMock(return_value=fake)
    ctx.publisher_cbor("sensor/data")
    kwargs = CborPublisher._declare.call_args.kwargs
    assert kwargs["schema_uri"] == "bubbaloop://emb/emb/sensor/data@v1"


def test_publisher_cbor_respects_explicit_empty_schema_uri():
    """Empty string is a valid override — consumers may rely on it for
    backward compat."""
    from bubbaloop_sdk.publisher import CborPublisher
    ctx = _make_context("bot", instance_name="emb")
    fake = MagicMock()
    CborPublisher._declare = MagicMock(return_value=fake)
    ctx.publisher_cbor("sensor/data", schema_uri="")
    assert CborPublisher._declare.call_args.kwargs["schema_uri"] == ""


def test_publisher_json_default_schema_uri_from_ctx():
    from bubbaloop_sdk.publisher import JsonPublisher
    ctx = _make_context("bot", instance_name="emb")
    fake = MagicMock()
    JsonPublisher._declare = MagicMock(return_value=fake)
    ctx.publisher_json("events")
    kwargs = JsonPublisher._declare.call_args.kwargs
    assert kwargs["schema_uri"] == "bubbaloop://emb/emb/events@v1"


def test_publisher_raw_absolute_uses_unscoped_key_and_local_flag():
    from bubbaloop_sdk import publisher as pub_mod
    ctx = _make_context("bot", instance_name="emb")
    pub_mod.RawPublisher._declare = MagicMock(return_value="pub")
    ctx.publisher_raw_absolute("frames", local=True)
    pub_mod.RawPublisher._declare.assert_called_once_with(
        ctx.session, "bubbaloop/local/bot/frames", local=True
    )


# ---------------------------------------------------------------------------
# Subscribers default to ABSOLUTE keys (no instance scoping)
# ---------------------------------------------------------------------------

def test_subscribe_uses_absolute_key_not_instance_scoped(monkeypatch):
    """subscribe() must NOT prepend instance_name. The 99% case is reading
    an upstream node's key — passing 'rtsp_camera/raw' should resolve to
    bubbaloop/global/{machine}/rtsp_camera/raw, NOT
    bubbaloop/global/{machine}/{me}/rtsp_camera/raw."""
    from bubbaloop_sdk import subscriber as sub_mod
    ctx = _make_context("bot", instance_name="me")
    captured = {}

    class _FakeSub:
        def __init__(self, session, key):
            captured["key"] = key

    monkeypatch.setattr(sub_mod, "CborSubscriber", _FakeSub)
    ctx.subscribe("rtsp_camera/raw")
    assert captured["key"] == "bubbaloop/global/bot/rtsp_camera/raw"


def test_subscribe_local_uses_absolute_local_key(monkeypatch):
    from bubbaloop_sdk import subscriber as sub_mod
    ctx = _make_context("bot", instance_name="me")
    captured = {}

    class _FakeSub:
        def __init__(self, session, key):
            captured["key"] = key

    monkeypatch.setattr(sub_mod, "CborSubscriber", _FakeSub)
    ctx.subscribe("rtsp_camera/raw", local=True)
    assert captured["key"] == "bubbaloop/local/bot/rtsp_camera/raw"


def test_subscribe_raw_uses_absolute_key_not_instance_scoped(monkeypatch):
    from bubbaloop_sdk import subscriber as sub_mod
    ctx = _make_context("bot", instance_name="me")
    captured = {}

    class _FakeRawSub:
        def __init__(self, session, key):
            captured["key"] = key

    monkeypatch.setattr(sub_mod, "RawSubscriber", _FakeRawSub)
    ctx.subscribe_raw("upstream/frames", local=True)
    assert captured["key"] == "bubbaloop/local/bot/upstream/frames"


def test_subscribe_shared_no_longer_exists():
    """The old subscribe_shared / subscribe_raw_shared helpers were removed —
    subscribe() is now absolute by default."""
    ctx = _make_context("bot", instance_name="me")
    assert not hasattr(ctx, "subscribe_shared")
    assert not hasattr(ctx, "subscribe_raw_shared")


# ---------------------------------------------------------------------------
# _resolve_topic()
# ---------------------------------------------------------------------------

def test_resolve_topic_global_auto_scoped():
    ctx = _make_context("bot", instance_name="cam")
    assert ctx._resolve_topic("data", False) == "bubbaloop/global/bot/cam/data"


def test_resolve_topic_local_auto_scoped():
    ctx = _make_context("bot", instance_name="cam")
    assert ctx._resolve_topic("raw", True) == "bubbaloop/local/bot/cam/raw"


def test_global_and_local_share_suffix_auto_scoped():
    ctx = _make_context("edge_42", instance_name="n1")
    global_topic = ctx.topic("sensor/data")
    local_topic = ctx.local_topic("sensor/data")
    assert global_topic.endswith("edge_42/n1/sensor/data")
    assert local_topic.endswith("edge_42/n1/sensor/data")
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
    from bubbaloop_sdk import CborPublisher, JsonPublisher
    assert CborPublisher is not None
    assert JsonPublisher is not None


def test_import_subscribers():
    from bubbaloop_sdk import CborSubscriber, Envelope, RawSubscriber
    assert CborSubscriber is not None
    assert RawSubscriber is not None
    assert Envelope is not None


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
# CborPublisher.put()
# ---------------------------------------------------------------------------

def test_cbor_publisher_wraps_dict_in_envelope():
    from bubbaloop_sdk.publisher import CborPublisher
    mock_pub = MagicMock()
    pub = CborPublisher(mock_pub, source_instance="cam_a", schema_uri="bubbaloop://cam/v1")
    pub.put({"temperature": 22.5})
    decoded = cbor2.loads(mock_pub.put.call_args[0][0])
    assert set(decoded.keys()) == {"header", "body"}
    assert decoded["body"] == {"temperature": 22.5}
    assert decoded["header"]["schema_uri"] == "bubbaloop://cam/v1"
    assert decoded["header"]["source_instance"] == "cam_a"
    assert decoded["header"]["monotonic_seq"] == 0
    assert isinstance(decoded["header"]["ts_ns"], int)
    assert decoded["header"]["ts_ns"] > 0


def test_cbor_publisher_monotonic_seq_increments():
    from bubbaloop_sdk.publisher import CborPublisher
    mock_pub = MagicMock()
    pub = CborPublisher(mock_pub, source_instance="cam_a")
    pub.put({"a": 1})
    pub.put({"a": 2})
    pub.put({"a": 3})
    seqs = [cbor2.loads(call.args[0])["header"]["monotonic_seq"]
            for call in mock_pub.put.call_args_list]
    assert seqs == [0, 1, 2]


def test_cbor_publisher_default_schema_uri_is_empty():
    from bubbaloop_sdk.publisher import CborPublisher
    mock_pub = MagicMock()
    CborPublisher(mock_pub, source_instance="cam_a").put({"x": 1})
    decoded = cbor2.loads(mock_pub.put.call_args[0][0])
    assert decoded["header"]["schema_uri"] == ""


def test_cbor_publisher_passthrough_bytes_skips_envelope():
    """Pre-encoded bytes are treated as final wire payload — no envelope wrap."""
    from bubbaloop_sdk.publisher import CborPublisher
    mock_pub = MagicMock()
    CborPublisher(mock_pub).put(b"\x01\x02\x03")
    mock_pub.put.assert_called_once_with(b"\x01\x02\x03")


def test_cbor_publisher_serializes_list_in_envelope():
    from bubbaloop_sdk.publisher import CborPublisher
    mock_pub = MagicMock()
    CborPublisher(mock_pub).put([1, 2, 3])
    decoded = cbor2.loads(mock_pub.put.call_args[0][0])
    assert decoded["body"] == [1, 2, 3]


# ---------------------------------------------------------------------------
# CborSubscriber.recv()
# ---------------------------------------------------------------------------

def test_cbor_subscriber_decodes_cbor_passthrough_when_not_enveloped():
    """Non-enveloped CBOR (e.g. from a non-SDK publisher) returns SimpleNamespace."""
    from bubbaloop_sdk.subscriber import CborSubscriber
    sub = object.__new__(CborSubscriber)
    sub._undeclared = False

    mock_sample = MagicMock()
    mock_sample.encoding.__str__ = lambda self: "application/cbor"
    mock_sample.payload = cbor2.dumps({"x": 1, "y": 2})

    sub._sub = MagicMock()
    sub._sub.recv.return_value = mock_sample

    result = sub.recv()
    assert result.x == 1
    assert result.y == 2


def test_cbor_subscriber_unwraps_envelope_to_envelope_dataclass():
    from bubbaloop_sdk.subscriber import CborSubscriber, Envelope
    sub = object.__new__(CborSubscriber)
    sub._undeclared = False

    payload = {
        "header": {
            "schema_uri": "bubbaloop://cam/v1",
            "source_instance": "tapo_terrace",
            "monotonic_seq": 7,
            "ts_ns": 1234567890,
        },
        "body": {"width": 640, "height": 480, "data": b"\x00\x01"},
    }
    mock_sample = MagicMock()
    mock_sample.encoding.__str__ = lambda self: "application/cbor"
    mock_sample.payload = cbor2.dumps(payload)

    sub._sub = MagicMock()
    sub._sub.recv.return_value = mock_sample

    result = sub.recv()
    assert isinstance(result, Envelope)
    assert result.body.width == 640
    assert result.body.height == 480
    assert result.header.schema_uri == "bubbaloop://cam/v1"
    assert result.header.source_instance == "tapo_terrace"
    assert result.header.monotonic_seq == 7
    assert result.header.ts_ns == 1234567890


def test_cbor_subscriber_does_not_wrap_payload_with_extra_keys():
    """Defensive: a body that *coincidentally* has 'header' and 'body' keys
    but ALSO has other keys is NOT treated as an envelope."""
    from bubbaloop_sdk.subscriber import CborSubscriber, Envelope
    sub = object.__new__(CborSubscriber)
    sub._undeclared = False

    payload = {"header": {"x": 1}, "body": {"y": 2}, "extra": 3}
    mock_sample = MagicMock()
    mock_sample.encoding.__str__ = lambda self: "application/cbor"
    mock_sample.payload = cbor2.dumps(payload)

    sub._sub = MagicMock()
    sub._sub.recv.return_value = mock_sample

    result = sub.recv()
    assert not isinstance(result, Envelope)
    assert result.extra == 3


def test_cbor_envelope_roundtrip_publisher_to_subscriber():
    """Encode through CborPublisher, decode through CborSubscriber's logic."""
    from bubbaloop_sdk.publisher import CborPublisher
    from bubbaloop_sdk.subscriber import CborSubscriber, Envelope

    mock_pub = MagicMock()
    CborPublisher(mock_pub, source_instance="probe", schema_uri="bubbaloop://probe/v1").put(
        {"hello": "world"}
    )
    wire_bytes = mock_pub.put.call_args[0][0]

    sub = object.__new__(CborSubscriber)
    sub._undeclared = False
    mock_sample = MagicMock()
    mock_sample.encoding.__str__ = lambda self: "application/cbor"
    mock_sample.payload = wire_bytes
    sub._sub = MagicMock()
    sub._sub.recv.return_value = mock_sample

    result = sub.recv()
    assert isinstance(result, Envelope)
    assert result.body.hello == "world"
    assert result.header.schema_uri == "bubbaloop://probe/v1"
    assert result.header.source_instance == "probe"
    assert result.header.monotonic_seq == 0


def test_cbor_subscriber_decodes_json():
    from bubbaloop_sdk.subscriber import CborSubscriber
    sub = object.__new__(CborSubscriber)
    sub._undeclared = False

    mock_sample = MagicMock()
    mock_sample.encoding.__str__ = lambda self: "application/json"
    mock_sample.payload = json.dumps({"temp": 42}).encode()

    sub._sub = MagicMock()
    sub._sub.recv.return_value = mock_sample

    result = sub.recv()
    assert result == {"temp": 42}


def test_cbor_subscriber_returns_bytes_for_unknown():
    from bubbaloop_sdk.subscriber import CborSubscriber
    sub = object.__new__(CborSubscriber)
    sub._undeclared = False

    mock_sample = MagicMock()
    mock_sample.encoding.__str__ = lambda self: "application/octet-stream"
    mock_sample.payload = b"\xde\xad\xbe\xef"

    sub._sub = MagicMock()
    sub._sub.recv.return_value = mock_sample

    result = sub.recv()
    assert result == b"\xde\xad\xbe\xef"


# ---------------------------------------------------------------------------
# JsonPublisher.put()
# ---------------------------------------------------------------------------

def test_json_publisher_wraps_dict_in_envelope():
    from bubbaloop_sdk.publisher import JsonPublisher
    mock_pub = MagicMock()
    pub = JsonPublisher(mock_pub, source_instance="cam_a", schema_uri="bubbaloop://cam/v1")
    pub.put({"temperature": 22.5})
    decoded = json.loads(mock_pub.put.call_args[0][0])
    assert set(decoded.keys()) == {"header", "body"}
    assert decoded["body"] == {"temperature": 22.5}
    assert decoded["header"]["schema_uri"] == "bubbaloop://cam/v1"
    assert decoded["header"]["source_instance"] == "cam_a"
    assert decoded["header"]["monotonic_seq"] == 0


def test_json_publisher_monotonic_seq_increments():
    from bubbaloop_sdk.publisher import JsonPublisher
    mock_pub = MagicMock()
    pub = JsonPublisher(mock_pub, source_instance="cam_a")
    pub.put({"a": 1})
    pub.put({"a": 2})
    seqs = [json.loads(call.args[0])["header"]["monotonic_seq"]
            for call in mock_pub.put.call_args_list]
    assert seqs == [0, 1]


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


def test_cbor_subscriber_unwraps_json_envelope():
    """JSON payloads now carry the same ``{header, body}`` envelope as CBOR
    — ``recv()`` returns an :class:`Envelope` for them."""
    from bubbaloop_sdk.subscriber import CborSubscriber, Envelope
    sub = object.__new__(CborSubscriber)
    sub._undeclared = False
    envelope = {
        "header": {
            "schema_uri": "bubbaloop://embedder/v1",
            "source_instance": "tapo_terrace_embedder",
            "monotonic_seq": 3,
            "ts_ns": 123,
        },
        "body": {"dim": 768, "embedding": [0.1, 0.2]},
    }
    mock_sample = MagicMock()
    mock_sample.encoding.__str__ = lambda self: "application/json"
    mock_sample.payload = json.dumps(envelope).encode()
    sub._sub = MagicMock()
    sub._sub.recv.return_value = mock_sample
    result = sub.recv()
    assert isinstance(result, Envelope)
    assert result.body.dim == 768
    assert result.header.schema_uri == "bubbaloop://embedder/v1"
    assert result.header.monotonic_seq == 3


def test_json_envelope_roundtrip_publisher_to_subscriber():
    from bubbaloop_sdk.publisher import JsonPublisher
    from bubbaloop_sdk.subscriber import CborSubscriber, Envelope

    mock_pub = MagicMock()
    JsonPublisher(mock_pub, source_instance="probe", schema_uri="bubbaloop://probe/v1").put(
        {"hello": "world"}
    )
    wire_bytes = mock_pub.put.call_args[0][0]

    sub = object.__new__(CborSubscriber)
    sub._undeclared = False
    mock_sample = MagicMock()
    mock_sample.encoding.__str__ = lambda self: "application/json"
    mock_sample.payload = wire_bytes
    sub._sub = MagicMock()
    sub._sub.recv.return_value = mock_sample

    result = sub.recv()
    assert isinstance(result, Envelope)
    assert result.body.hello == "world"
    assert result.header.schema_uri == "bubbaloop://probe/v1"


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

def _make_context(machine_id: str, instance_name: str | None = None):
    """Build a NodeContext without opening a Zenoh session.

    By default ``instance_name`` is None (legacy fallback); pass an explicit
    name to test the auto-scoping behavior.
    """
    from bubbaloop_sdk.context import NodeContext
    ctx = object.__new__(NodeContext)
    ctx.session = MagicMock()
    ctx.machine_id = machine_id
    ctx.instance_name = instance_name
    ctx._shutdown = threading.Event()
    return ctx
