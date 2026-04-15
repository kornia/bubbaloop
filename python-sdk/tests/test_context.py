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
    assert ctx.topic("camera/front/compressed") == ("bubbaloop/global/jetson_orin/camera/front/compressed")


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
    from bubbaloop_sdk import JsonPublisher, ProtoPublisher

    assert ProtoPublisher is not None
    assert JsonPublisher is not None


def test_import_subscribers():
    from bubbaloop_sdk import ProtoSubscriber, RawSubscriber

    assert ProtoSubscriber is not None
    assert RawSubscriber is not None


def test_import_callback_subscribers():
    from bubbaloop_sdk import CallbackSubscriber, RawCallbackSubscriber

    assert CallbackSubscriber is not None
    assert RawCallbackSubscriber is not None


def test_import_callback_subscribers_with_workers():
    from bubbaloop_sdk import CallbackSubscriber, RawCallbackSubscriber

    assert CallbackSubscriber is not None
    assert RawCallbackSubscriber is not None


def test_import_async_queryable():
    from bubbaloop_sdk import Queryable

    assert Queryable is not None


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


def test_raw_subscriber_recv_returns_bytes():
    """RawSubscriber.recv() returns bytes from sample payload."""
    from bubbaloop_sdk.subscriber import RawSubscriber

    fake_sample = MagicMock()
    fake_sample.payload = b"\xde\xad\xbe\xef"

    mock_sub = MagicMock()
    mock_sub.recv.return_value = fake_sample

    mock_session = MagicMock()
    mock_session.declare_subscriber.return_value = mock_sub

    sub = RawSubscriber(mock_session, "test/topic")
    result = sub.recv()
    assert result == b"\xde\xad\xbe\xef"


# ---------------------------------------------------------------------------
# RawSubscriber — undeclare unblocks recv()
# ---------------------------------------------------------------------------


def test_raw_subscriber_undeclare_is_idempotent():
    """undeclare() can be called twice without error."""
    from bubbaloop_sdk.subscriber import RawSubscriber

    mock_sub = MagicMock()
    mock_session = MagicMock()
    mock_session.declare_subscriber.return_value = mock_sub

    sub = RawSubscriber(mock_session, "test/topic")
    sub.undeclare()
    sub.undeclare()  # second call is a no-op
    mock_sub.undeclare.assert_called_once()


# ---------------------------------------------------------------------------
# CallbackSubscriber(max_workers=4) / RawCallbackSubscriber(max_workers=4) — _closing flag
# ---------------------------------------------------------------------------


def test_callback_subscriber_with_workers_drops_after_undeclare():
    """Callbacks arriving after undeclare() are silently dropped."""
    from bubbaloop_sdk.subscriber import CallbackSubscriber

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(topic, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare
    received = []
    called = threading.Event()

    def handler(msg):
        received.append(msg)
        called.set()

    sub = CallbackSubscriber(mock_session, "test/topic", handler, MagicMock(), max_workers=4)
    sub.undeclare()

    # Simulate a late-arriving Zenoh callback after undeclare.
    # _closing is already set so _wrap returns early — handler is never submitted.
    fake_sample = MagicMock()
    captured_handler[0](fake_sample)  # must not raise

    # Give the executor no chance to run (it's shut down); assert immediately.
    assert not called.wait(timeout=0.1), "handler should not be called after undeclare()"
    assert received == []


def test_raw_callback_subscriber_with_workers_drops_after_undeclare():
    """Callbacks arriving after undeclare() are silently dropped."""
    from bubbaloop_sdk.subscriber import RawCallbackSubscriber

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(key_expr, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare
    received = []
    called = threading.Event()

    def handler(sample):
        received.append(sample)
        called.set()

    sub = RawCallbackSubscriber(mock_session, "test/**", handler, max_workers=4)
    sub.undeclare()

    fake_sample = MagicMock()
    captured_handler[0](fake_sample)  # must not raise

    assert not called.wait(timeout=0.1), "handler should not be called after undeclare()"
    assert received == []


def test_async_queryable_drops_after_undeclare():
    """Queries arriving after undeclare() are silently dropped (thread pool mode)."""
    from bubbaloop_sdk.subscriber import Queryable

    mock_session = MagicMock()
    captured_wrapper = []

    def fake_declare(key_expr, wrapper):
        captured_wrapper.append(wrapper)
        return MagicMock()

    mock_session.declare_queryable.side_effect = fake_declare
    received = []
    called = threading.Event()

    def handler(query):
        received.append(query)
        called.set()

    aq = Queryable(mock_session, "test/topic", handler, max_workers=4)
    aq.undeclare()

    fake_query = MagicMock()
    captured_wrapper[0](fake_query)  # must not raise

    assert not called.wait(timeout=0.1), "handler should not be called after undeclare()"
    assert received == []


# ---------------------------------------------------------------------------
# CallbackSubscriber
# ---------------------------------------------------------------------------


def test_callback_subscriber_calls_handler_with_decoded():
    """Handler receives whatever registry.decode() returns."""
    from bubbaloop_sdk.subscriber import CallbackSubscriber

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(topic, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare

    mock_registry = MagicMock()
    mock_registry.decode.return_value = {"temperature": 22.5}

    received = []
    sub = CallbackSubscriber(mock_session, "test/topic", lambda msg: received.append(msg), mock_registry)

    fake_sample = MagicMock()
    captured_handler[0](fake_sample)

    assert received == [{"temperature": 22.5}]
    mock_registry.decode.assert_called_once_with(fake_sample)
    sub.undeclare()


def test_callback_subscriber_passes_sample_to_registry():
    """CallbackSubscriber passes the zenoh.Sample to registry.decode()."""
    from bubbaloop_sdk.subscriber import CallbackSubscriber

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(topic, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare

    mock_registry = MagicMock()
    mock_registry.decode.return_value = "decoded_proto"

    received = []
    sub = CallbackSubscriber(mock_session, "test/topic", lambda msg: received.append(msg), mock_registry)

    fake_sample = MagicMock()
    captured_handler[0](fake_sample)

    assert received == ["decoded_proto"]
    mock_registry.decode.assert_called_once_with(fake_sample)
    sub.undeclare()


def test_callback_subscriber_undeclare():
    """undeclare() calls undeclare on the underlying zenoh subscriber."""
    from bubbaloop_sdk.subscriber import CallbackSubscriber

    mock_session = MagicMock()
    mock_sub = MagicMock()
    mock_session.declare_subscriber.return_value = mock_sub
    sub = CallbackSubscriber(mock_session, "test/topic", lambda msg: None, MagicMock())
    sub.undeclare()
    mock_sub.undeclare.assert_called_once()


# ---------------------------------------------------------------------------
# RawCallbackSubscriber
# ---------------------------------------------------------------------------


def test_raw_callback_subscriber_passes_sample():
    """Handler receives the raw zenoh.Sample object."""
    from bubbaloop_sdk.subscriber import RawCallbackSubscriber

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(key_expr, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare
    received = []
    sub = RawCallbackSubscriber(mock_session, "test/**", lambda s: received.append(s))

    fake_sample = MagicMock()
    captured_handler[0](fake_sample)

    assert received == [fake_sample]
    sub.undeclare()


def test_raw_callback_subscriber_undeclare():
    """undeclare() calls undeclare on the underlying zenoh subscriber."""
    from bubbaloop_sdk.subscriber import RawCallbackSubscriber

    mock_session = MagicMock()
    mock_sub = MagicMock()
    mock_session.declare_subscriber.return_value = mock_sub
    sub = RawCallbackSubscriber(mock_session, "test/**", lambda s: None)
    sub.undeclare()
    mock_sub.undeclare.assert_called_once()


def test_callback_subscriber_undeclare_is_idempotent():
    """undeclare() can be called twice without error."""
    from bubbaloop_sdk.subscriber import CallbackSubscriber

    mock_session = MagicMock()
    mock_session.declare_subscriber.return_value = MagicMock()
    sub = CallbackSubscriber(mock_session, "test/topic", lambda msg: None, MagicMock())
    sub.undeclare()
    sub.undeclare()  # second call is a no-op
    mock_session.declare_subscriber.return_value.undeclare.assert_called_once()


def test_callback_subscriber_with_workers_undeclare_is_idempotent():
    """undeclare() can be called twice without error."""
    from bubbaloop_sdk.subscriber import CallbackSubscriber

    mock_session = MagicMock()
    mock_sub = MagicMock()
    mock_session.declare_subscriber.return_value = mock_sub
    sub = CallbackSubscriber(mock_session, "test/topic", lambda msg: None, MagicMock(), max_workers=4)
    sub.undeclare()
    sub.undeclare()  # second call is a no-op
    mock_sub.undeclare.assert_called_once()


# ---------------------------------------------------------------------------
# CallbackSubscriber with max_workers (thread pool mode)
# ---------------------------------------------------------------------------


def test_callback_subscriber_with_workers_calls_handler_in_thread_pool():
    """Handler is called asynchronously via thread pool when max_workers is set."""
    import threading

    from bubbaloop_sdk.subscriber import CallbackSubscriber

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(topic, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare

    mock_registry = MagicMock()
    mock_registry.decode.return_value = b"\xca\xfe"

    received = []
    event = threading.Event()

    def slow_handler(msg):
        received.append(msg)
        event.set()

    sub = CallbackSubscriber(mock_session, "test/topic", slow_handler, mock_registry, max_workers=4)

    fake_sample = MagicMock()
    captured_handler[0](fake_sample)

    assert event.wait(timeout=2.0), "handler was not called within 2s"
    assert received == [b"\xca\xfe"]
    sub.undeclare()


def test_callback_subscriber_with_workers_passes_sample_to_registry():
    """CallbackSubscriber with max_workers passes the zenoh.Sample to registry.decode()."""
    import threading

    from bubbaloop_sdk.subscriber import CallbackSubscriber

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(topic, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare

    mock_registry = MagicMock()
    mock_registry.decode.return_value = "decoded"
    received = []
    event = threading.Event()

    def handler(msg):
        received.append(msg)
        event.set()

    sub = CallbackSubscriber(mock_session, "test/topic", handler, mock_registry, max_workers=4)

    fake_sample = MagicMock()
    captured_handler[0](fake_sample)

    assert event.wait(timeout=2.0)
    assert received == ["decoded"]
    mock_registry.decode.assert_called_once_with(fake_sample)
    sub.undeclare()


def test_raw_callback_subscriber_with_workers_passes_sample():
    """RawCallbackSubscriber with max_workers handler receives raw zenoh.Sample."""
    import threading

    from bubbaloop_sdk.subscriber import RawCallbackSubscriber

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(key_expr, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare
    received = []
    event = threading.Event()

    def handler(sample):
        received.append(sample)
        event.set()

    sub = RawCallbackSubscriber(mock_session, "test/**", handler, max_workers=4)

    fake_sample = MagicMock()
    captured_handler[0](fake_sample)

    assert event.wait(timeout=2.0)
    assert received == [fake_sample]
    sub.undeclare()


def test_callback_subscriber_with_workers_undeclare():
    """undeclare() shuts down executor and undeclares underlying sub."""
    from bubbaloop_sdk.subscriber import CallbackSubscriber

    mock_session = MagicMock()
    mock_sub = MagicMock()
    mock_session.declare_subscriber.return_value = mock_sub
    sub = CallbackSubscriber(mock_session, "test/topic", lambda msg: None, MagicMock(), max_workers=4)
    sub.undeclare()
    mock_sub.undeclare.assert_called_once()


def test_raw_callback_subscriber_with_workers_undeclare():
    """undeclare() shuts down executor and undeclares underlying sub."""
    from bubbaloop_sdk.subscriber import RawCallbackSubscriber

    mock_session = MagicMock()
    mock_sub = MagicMock()
    mock_session.declare_subscriber.return_value = mock_sub
    sub = RawCallbackSubscriber(mock_session, "test/**", lambda s: None, max_workers=4)
    sub.undeclare()
    mock_sub.undeclare.assert_called_once()


# ---------------------------------------------------------------------------
# NodeContext.queryable() and queryable_raw()
# ---------------------------------------------------------------------------


def test_queryable_uses_topic_prefix():
    """queryable() declares at bubbaloop/global/{machine_id}/{suffix}."""
    ctx = _make_context("bot")

    def handler(q):
        pass

    qbl = ctx.queryable("command", handler)
    try:
        called_topic = ctx.session.declare_queryable.call_args[0][0]
        assert called_topic == "bubbaloop/global/bot/command"
    finally:
        qbl.undeclare()


def test_queryable_raw_uses_literal_key_expr():
    """queryable_raw() declares at the literal key expression provided."""
    ctx = _make_context("bot")

    def handler(q):
        pass

    qbl = ctx.queryable_raw("bubbaloop/**/schema", handler)
    try:
        called_topic = ctx.session.declare_queryable.call_args[0][0]
        assert called_topic == "bubbaloop/**/schema"
    finally:
        qbl.undeclare()


def test_queryable_returns_async_queryable():
    """queryable() returns Queryable."""
    from bubbaloop_sdk.subscriber import Queryable

    ctx = _make_context("bot")
    result = ctx.queryable("command", lambda q: None)
    try:
        assert isinstance(result, Queryable)
    finally:
        result.undeclare()


# ---------------------------------------------------------------------------
# NodeContext.queryable(max_workers) and queryable_raw(max_workers)
# ---------------------------------------------------------------------------


def test_queryable_with_workers_uses_topic_prefix():
    """queryable(max_workers=4) declares at topic(suffix)."""
    ctx = _make_context("bot")

    def handler(q):
        pass

    qbl = ctx.queryable("command", handler, max_workers=4)
    try:
        called_topic = ctx.session.declare_queryable.call_args[0][0]
        assert called_topic == "bubbaloop/global/bot/command"
    finally:
        qbl.undeclare()


def test_queryable_with_workers_wraps_handler_in_executor():
    """queryable(max_workers=4) wraps handler so Zenoh thread is freed."""
    import threading

    ctx = _make_context("bot")
    captured_wrapper = []

    def fake_declare(topic, wrapper):
        captured_wrapper.append(wrapper)
        return MagicMock()

    ctx.session.declare_queryable.side_effect = fake_declare

    received = []
    event = threading.Event()

    def slow_handler(query):
        received.append(query)
        event.set()

    qbl = ctx.queryable("command", slow_handler, max_workers=4)

    try:
        fake_query = MagicMock()
        captured_wrapper[0](fake_query)  # Zenoh calls the wrapper

        assert event.wait(timeout=2.0), "handler not called within 2s"
        assert received == [fake_query]
    finally:
        qbl.undeclare()


def test_queryable_with_workers_returns_async_queryable():
    """queryable(max_workers=4) returns Queryable."""
    from bubbaloop_sdk.subscriber import Queryable

    ctx = _make_context("bot")
    qbl = ctx.queryable("command", lambda q: None, max_workers=4)
    try:
        assert isinstance(qbl, Queryable)
    finally:
        qbl.undeclare()


def test_queryable_raw_with_workers_uses_literal_key_expr():
    """queryable_raw(max_workers=4) declares at the literal key expression."""
    ctx = _make_context("bot")
    qbl = ctx.queryable_raw("bubbaloop/**/schema", lambda q: None, max_workers=4)
    try:
        called_topic = ctx.session.declare_queryable.call_args[0][0]
        assert called_topic == "bubbaloop/**/schema"
    finally:
        qbl.undeclare()


def test_queryable_raw_with_workers_wraps_handler_in_executor():
    """queryable_raw(max_workers=4) wraps handler in thread pool."""
    import threading

    ctx = _make_context("bot")
    captured_wrapper = []

    def fake_declare(key_expr, wrapper):
        captured_wrapper.append(wrapper)
        return MagicMock()

    ctx.session.declare_queryable.side_effect = fake_declare

    received = []
    event = threading.Event()

    def handler(query):
        received.append(query)
        event.set()

    qbl = ctx.queryable_raw("bubbaloop/**/schema", handler, max_workers=4)

    try:
        fake_query = MagicMock()
        captured_wrapper[0](fake_query)

        assert event.wait(timeout=2.0), "handler not called within 2s"
        assert received == [fake_query]
    finally:
        qbl.undeclare()


def test_async_queryable_undeclare():
    """Queryable.undeclare() undeclares queryable then shuts executor."""
    from bubbaloop_sdk.subscriber import Queryable

    mock_session = MagicMock()
    mock_qbl = MagicMock()
    mock_session.declare_queryable.return_value = mock_qbl
    aq = Queryable(mock_session, "test/topic", lambda q: None)
    aq.undeclare()
    mock_qbl.undeclare.assert_called_once()


# ---------------------------------------------------------------------------
# NodeContext.subscriber_callback()
# ---------------------------------------------------------------------------


def test_subscriber_callback_uses_topic_prefix():
    """subscriber_callback() declares at topic(suffix)."""
    ctx = _make_context("bot")
    ctx._schema_registry = MagicMock()
    ctx.subscriber_callback("sensor/data", lambda msg: None)
    called_topic = ctx.session.declare_subscriber.call_args[0][0]
    assert called_topic == "bubbaloop/global/bot/sensor/data"


def test_subscriber_raw_callback_uses_literal_key_expr():
    """subscriber_raw_callback() declares at the literal key expression."""
    ctx = _make_context("bot")
    ctx.subscriber_raw_callback("bubbaloop/**/health", lambda s: None)
    called_topic = ctx.session.declare_subscriber.call_args[0][0]
    assert called_topic == "bubbaloop/**/health"


def test_subscriber_callback_with_workers_uses_topic_prefix():
    """subscriber_callback() with max_workers declares at topic(suffix)."""
    ctx = _make_context("bot")
    ctx._schema_registry = MagicMock()
    sub = ctx.subscriber_callback("sensor/data", lambda msg: None, max_workers=4)
    try:
        called_topic = ctx.session.declare_subscriber.call_args[0][0]
        assert called_topic == "bubbaloop/global/bot/sensor/data"
    finally:
        sub.undeclare()


def test_subscriber_raw_callback_with_workers_uses_literal_key_expr():
    """subscriber_raw_callback() with max_workers declares at literal key expression."""
    ctx = _make_context("bot")
    sub = ctx.subscriber_raw_callback("bubbaloop/**/health", lambda s: None, max_workers=4)
    try:
        called_topic = ctx.session.declare_subscriber.call_args[0][0]
        assert called_topic == "bubbaloop/**/health"
    finally:
        sub.undeclare()


# ---------------------------------------------------------------------------
# NodeContext.publisher_json() / publisher_proto() via context
# ---------------------------------------------------------------------------


def test_publisher_json_uses_topic_prefix():
    """publisher_json() declares at topic(suffix)."""
    ctx = _make_context("bot")
    ctx.publisher_json("weather/current")
    called_topic = ctx.session.declare_publisher.call_args[0][0]
    assert called_topic == "bubbaloop/global/bot/weather/current"


def test_publisher_proto_uses_topic_prefix():
    """publisher_proto() declares at topic(suffix)."""
    ctx = _make_context("bot")
    fake_class = MagicMock()
    fake_class.DESCRIPTOR.full_name = "my.SensorData"
    ctx.publisher_proto("sensor/data", fake_class)
    called_topic = ctx.session.declare_publisher.call_args[0][0]
    assert called_topic == "bubbaloop/global/bot/sensor/data"


# ---------------------------------------------------------------------------
# NodeContext.close() and context manager
# ---------------------------------------------------------------------------


def test_close_calls_session_close():
    """close() calls session.close()."""
    ctx = _make_context("bot")
    ctx.close()
    ctx.session.close.assert_called_once()


def test_context_manager_calls_close():
    """__exit__ calls close() so the session is always cleaned up."""
    ctx = _make_context("bot")
    with ctx:
        pass
    ctx.session.close.assert_called_once()


# ---------------------------------------------------------------------------
# NodeContext.connect() — env var resolution
# ---------------------------------------------------------------------------


def test_connect_reads_machine_id_from_env(monkeypatch):
    """BUBBALOOP_MACHINE_ID env var sets ctx.machine_id."""
    import zenoh

    monkeypatch.setenv("BUBBALOOP_MACHINE_ID", "jetson_orin")

    monkeypatch.delenv("BUBBALOOP_ZENOH_ENDPOINT", raising=False)
    monkeypatch.setattr(zenoh, "open", lambda cfg: MagicMock())
    monkeypatch.setattr(zenoh, "Config", MagicMock)
    from bubbaloop_sdk.context import NodeContext

    ctx = NodeContext.connect()
    assert ctx.machine_id == "jetson_orin"


def test_connect_instance_name_override(monkeypatch):
    """instance_name kwarg overrides hostname fallback."""
    import zenoh

    monkeypatch.delenv("BUBBALOOP_MACHINE_ID", raising=False)

    monkeypatch.delenv("BUBBALOOP_ZENOH_ENDPOINT", raising=False)
    monkeypatch.setattr(zenoh, "open", lambda cfg: MagicMock())
    monkeypatch.setattr(zenoh, "Config", MagicMock)
    from bubbaloop_sdk.context import NodeContext

    ctx = NodeContext.connect(instance_name="tapo_entrance")
    assert ctx.instance_name == "tapo_entrance"


# ---------------------------------------------------------------------------
# RawSubscriber — undeclare() and iteration
# ---------------------------------------------------------------------------


def test_raw_subscriber_undeclare():
    """undeclare() calls undeclare on the underlying zenoh subscriber."""
    from bubbaloop_sdk.subscriber import RawSubscriber

    mock_session = MagicMock()
    mock_sub = MagicMock()
    mock_session.declare_subscriber.return_value = mock_sub
    sub = RawSubscriber(mock_session, "test/topic")
    sub.undeclare()
    mock_sub.undeclare.assert_called_once()


def test_raw_subscriber_declares_on_topic():
    """RawSubscriber declares a zenoh subscriber on the given topic."""
    from bubbaloop_sdk.subscriber import RawSubscriber

    mock_session = MagicMock()
    mock_session.declare_subscriber.return_value = MagicMock()
    RawSubscriber(mock_session, "test/topic")
    mock_session.declare_subscriber.assert_called_once_with("test/topic")


def test_raw_subscriber_undeclare_calls_sub_undeclare():
    """undeclare() calls undeclare on the underlying zenoh subscriber."""
    from bubbaloop_sdk.subscriber import RawSubscriber

    mock_sub = MagicMock()
    mock_session = MagicMock()
    mock_session.declare_subscriber.return_value = mock_sub
    sub = RawSubscriber(mock_session, "test/topic")
    sub.undeclare()
    mock_sub.undeclare.assert_called_once()


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
    from bubbaloop_sdk import GetSampleTimeout, get_sample

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
