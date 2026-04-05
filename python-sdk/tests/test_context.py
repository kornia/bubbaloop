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
    assert ctx.topic("camera/front/compressed") == ("bubbaloop/staging/jetson_orin/camera/front/compressed")


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
    from bubbaloop_sdk import JsonPublisher, ProtoPublisher

    assert ProtoPublisher is not None
    assert JsonPublisher is not None


def test_import_subscribers():
    from bubbaloop_sdk import RawSubscriber, TypedSubscriber

    assert TypedSubscriber is not None
    assert RawSubscriber is not None


def test_import_callback_subscribers():
    from bubbaloop_sdk import CallbackSubscriber, RawCallbackSubscriber

    assert CallbackSubscriber is not None
    assert RawCallbackSubscriber is not None


def test_import_callback_subscribers_async():
    from bubbaloop_sdk import CallbackSubscriberAsync, RawCallbackSubscriberAsync

    assert CallbackSubscriberAsync is not None
    assert RawCallbackSubscriberAsync is not None


def test_import_async_queryable():
    from bubbaloop_sdk import AsyncQueryable

    assert AsyncQueryable is not None


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
# TypedSubscriber — queue-backed with timeout
# ---------------------------------------------------------------------------


def test_typed_subscriber_recv_returns_none_on_timeout():
    """recv(timeout) returns None when queue is empty within timeout."""
    from bubbaloop_sdk.subscriber import TypedSubscriber

    mock_session = MagicMock()
    mock_session.declare_subscriber.return_value = MagicMock()
    sub = TypedSubscriber(mock_session, "test/topic")
    result = sub.recv(timeout=0.05)
    assert result is None


def test_typed_subscriber_recv_returns_message_when_available():
    """recv() returns the message put into the queue by the callback."""
    from bubbaloop_sdk.subscriber import TypedSubscriber

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(topic, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare
    sub = TypedSubscriber(mock_session, "test/topic")

    # Simulate Zenoh delivering a sample
    fake_sample = MagicMock()
    fake_sample.payload.to_bytes.return_value = b"\x01\x02"
    captured_handler[0](fake_sample)

    result = sub.recv(timeout=1.0)
    assert result == b"\x01\x02"


def test_typed_subscriber_recv_decodes_proto():
    """recv() decodes with FromString when msg_class provided."""
    from bubbaloop_sdk.subscriber import TypedSubscriber

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(topic, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare

    fake_msg_class = MagicMock()
    fake_msg_class.FromString.return_value = "decoded"
    sub = TypedSubscriber(mock_session, "test/topic", msg_class=fake_msg_class)

    fake_sample = MagicMock()
    fake_sample.payload.to_bytes.return_value = b"\x01"
    captured_handler[0](fake_sample)

    result = sub.recv(timeout=1.0)
    assert result == "decoded"
    fake_msg_class.FromString.assert_called_once_with(b"\x01")


def test_raw_subscriber_recv_returns_none_on_timeout():
    """RawSubscriber.recv(timeout) returns None when queue is empty."""
    from bubbaloop_sdk.subscriber import RawSubscriber

    mock_session = MagicMock()
    mock_session.declare_subscriber.return_value = MagicMock()
    sub = RawSubscriber(mock_session, "test/topic")
    result = sub.recv(timeout=0.05)
    assert result is None


def test_raw_subscriber_recv_returns_sample():
    """RawSubscriber.recv() returns the raw zenoh.Sample."""
    from bubbaloop_sdk.subscriber import RawSubscriber

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(topic, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare
    sub = RawSubscriber(mock_session, "test/topic")

    fake_sample = MagicMock()
    captured_handler[0](fake_sample)

    result = sub.recv(timeout=1.0)
    assert result is fake_sample


# ---------------------------------------------------------------------------
# TypedSubscriber / RawSubscriber — undeclare unblocks recv()
# ---------------------------------------------------------------------------


def test_typed_subscriber_undeclare_unblocks_recv():
    """undeclare() unblocks a thread waiting in recv(timeout=None)."""
    from bubbaloop_sdk.subscriber import TypedSubscriber

    mock_session = MagicMock()
    mock_session.declare_subscriber.return_value = MagicMock()
    sub = TypedSubscriber(mock_session, "test/topic")

    result_holder = []

    def blocking_recv():
        result_holder.append(sub.recv(timeout=None))

    t = threading.Thread(target=blocking_recv)
    t.start()
    sub.undeclare()
    t.join(timeout=2.0)

    assert not t.is_alive(), "recv() did not unblock after undeclare()"
    assert result_holder == [None]


def test_raw_subscriber_undeclare_unblocks_recv():
    """undeclare() unblocks a thread waiting in recv(timeout=None)."""
    from bubbaloop_sdk.subscriber import RawSubscriber

    mock_session = MagicMock()
    mock_session.declare_subscriber.return_value = MagicMock()
    sub = RawSubscriber(mock_session, "test/topic")

    result_holder = []

    def blocking_recv():
        result_holder.append(sub.recv(timeout=None))

    t = threading.Thread(target=blocking_recv)
    t.start()
    sub.undeclare()
    t.join(timeout=2.0)

    assert not t.is_alive(), "recv() did not unblock after undeclare()"
    assert result_holder == [None]


def test_typed_subscriber_decode_happens_in_recv_not_callback():
    """FromString is called in recv(), not inside the Zenoh callback."""
    from bubbaloop_sdk.subscriber import TypedSubscriber

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(topic, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare
    decode_thread_ids = []
    callback_thread_id = []

    class FakeMsgClass:
        @staticmethod
        def FromString(data):
            decode_thread_ids.append(threading.current_thread().ident)
            return f"decoded:{data}"

    sub = TypedSubscriber(mock_session, "test/topic", msg_class=FakeMsgClass)

    def zenoh_callback():
        callback_thread_id.append(threading.current_thread().ident)
        fake_sample = MagicMock()
        fake_sample.payload.to_bytes.return_value = b"\x01"
        captured_handler[0](fake_sample)

    t = threading.Thread(target=zenoh_callback)
    t.start()
    t.join()

    result = sub.recv(timeout=1.0)

    assert result == "decoded:b'\\x01'"
    # Decode must NOT have happened on the Zenoh (callback) thread
    assert decode_thread_ids[0] != callback_thread_id[0]


# ---------------------------------------------------------------------------
# CallbackSubscriberAsync / RawCallbackSubscriberAsync — _closing flag
# ---------------------------------------------------------------------------


def test_callback_subscriber_async_drops_after_undeclare():
    """Callbacks arriving after undeclare() are silently dropped."""
    from bubbaloop_sdk.subscriber import CallbackSubscriberAsync

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

    sub = CallbackSubscriberAsync(mock_session, "test/topic", handler)
    sub.undeclare()

    # Simulate a late-arriving Zenoh callback after undeclare.
    # _closing is already set so _wrap returns early — handler is never submitted.
    fake_sample = MagicMock()
    fake_sample.payload.to_bytes.return_value = b"\xff"
    captured_handler[0](fake_sample)  # must not raise

    # Give the executor no chance to run (it's shut down); assert immediately.
    assert not called.wait(timeout=0.1), "handler should not be called after undeclare()"
    assert received == []


def test_raw_callback_subscriber_async_drops_after_undeclare():
    """Callbacks arriving after undeclare() are silently dropped."""
    from bubbaloop_sdk.subscriber import RawCallbackSubscriberAsync

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

    sub = RawCallbackSubscriberAsync(mock_session, "test/**", handler)
    sub.undeclare()

    fake_sample = MagicMock()
    captured_handler[0](fake_sample)  # must not raise

    assert not called.wait(timeout=0.1), "handler should not be called after undeclare()"
    assert received == []


def test_async_queryable_drops_after_undeclare():
    """Queries arriving after undeclare() are silently dropped."""
    from bubbaloop_sdk.subscriber import AsyncQueryable

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

    aq = AsyncQueryable(mock_session, "test/topic", handler)
    aq.undeclare()

    fake_query = MagicMock()
    captured_wrapper[0](fake_query)  # must not raise

    assert not called.wait(timeout=0.1), "handler should not be called after undeclare()"
    assert received == []


# ---------------------------------------------------------------------------
# CallbackSubscriber
# ---------------------------------------------------------------------------


def test_callback_subscriber_calls_handler_with_bytes():
    """Handler receives raw bytes when no msg_class provided."""
    from bubbaloop_sdk.subscriber import CallbackSubscriber

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(topic, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare
    received = []
    sub = CallbackSubscriber(mock_session, "test/topic", lambda msg: received.append(msg))

    fake_sample = MagicMock()
    fake_sample.payload.to_bytes.return_value = b"\xde\xad"
    captured_handler[0](fake_sample)

    assert received == [b"\xde\xad"]
    sub.undeclare()


def test_callback_subscriber_decodes_proto():
    """Handler receives decoded proto when msg_class provided."""
    from bubbaloop_sdk.subscriber import CallbackSubscriber

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(topic, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare

    fake_msg_class = MagicMock()
    fake_msg_class.FromString.return_value = "decoded_proto"
    received = []
    sub = CallbackSubscriber(mock_session, "test/topic", lambda msg: received.append(msg), msg_class=fake_msg_class)

    fake_sample = MagicMock()
    fake_sample.payload.to_bytes.return_value = b"\x01"
    captured_handler[0](fake_sample)

    assert received == ["decoded_proto"]
    fake_msg_class.FromString.assert_called_once_with(b"\x01")
    sub.undeclare()


def test_callback_subscriber_undeclare():
    """undeclare() calls undeclare on the underlying zenoh subscriber."""
    from bubbaloop_sdk.subscriber import CallbackSubscriber

    mock_session = MagicMock()
    mock_sub = MagicMock()
    mock_session.declare_subscriber.return_value = mock_sub
    sub = CallbackSubscriber(mock_session, "test/topic", lambda msg: None)
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


# ---------------------------------------------------------------------------
# CallbackSubscriberAsync
# ---------------------------------------------------------------------------


def test_callback_subscriber_async_calls_handler_in_thread_pool():
    """Handler is called asynchronously via thread pool."""
    import threading

    from bubbaloop_sdk.subscriber import CallbackSubscriberAsync

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(topic, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare
    received = []
    event = threading.Event()

    def slow_handler(msg):
        received.append(msg)
        event.set()

    sub = CallbackSubscriberAsync(mock_session, "test/topic", slow_handler)

    fake_sample = MagicMock()
    fake_sample.payload.to_bytes.return_value = b"\xca\xfe"
    captured_handler[0](fake_sample)

    assert event.wait(timeout=2.0), "handler was not called within 2s"
    assert received == [b"\xca\xfe"]
    sub.undeclare()


def test_callback_subscriber_async_decodes_proto():
    """Handler receives decoded proto when msg_class provided."""
    import threading

    from bubbaloop_sdk.subscriber import CallbackSubscriberAsync

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(topic, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare

    fake_msg_class = MagicMock()
    fake_msg_class.FromString.return_value = "decoded"
    received = []
    event = threading.Event()

    def handler(msg):
        received.append(msg)
        event.set()

    sub = CallbackSubscriberAsync(mock_session, "test/topic", handler, msg_class=fake_msg_class)

    fake_sample = MagicMock()
    fake_sample.payload.to_bytes.return_value = b"\x01"
    captured_handler[0](fake_sample)

    assert event.wait(timeout=2.0)
    assert received == ["decoded"]
    sub.undeclare()


def test_raw_callback_subscriber_async_passes_sample():
    """RawCallbackSubscriberAsync handler receives raw zenoh.Sample."""
    import threading

    from bubbaloop_sdk.subscriber import RawCallbackSubscriberAsync

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

    sub = RawCallbackSubscriberAsync(mock_session, "test/**", handler)

    fake_sample = MagicMock()
    captured_handler[0](fake_sample)

    assert event.wait(timeout=2.0)
    assert received == [fake_sample]
    sub.undeclare()


def test_callback_subscriber_async_undeclare():
    """undeclare() shuts down executor and undeclares underlying sub."""
    from bubbaloop_sdk.subscriber import CallbackSubscriberAsync

    mock_session = MagicMock()
    mock_sub = MagicMock()
    mock_session.declare_subscriber.return_value = mock_sub
    sub = CallbackSubscriberAsync(mock_session, "test/topic", lambda msg: None)
    sub.undeclare()
    mock_sub.undeclare.assert_called_once()


def test_raw_callback_subscriber_async_undeclare():
    """undeclare() shuts down executor and undeclares underlying sub."""
    from bubbaloop_sdk.subscriber import RawCallbackSubscriberAsync

    mock_session = MagicMock()
    mock_sub = MagicMock()
    mock_session.declare_subscriber.return_value = mock_sub
    sub = RawCallbackSubscriberAsync(mock_session, "test/**", lambda s: None)
    sub.undeclare()
    mock_sub.undeclare.assert_called_once()


# ---------------------------------------------------------------------------
# NodeContext.queryable() and queryable_raw()
# ---------------------------------------------------------------------------


def test_queryable_uses_topic_prefix():
    """queryable() declares at bubbaloop/{scope}/{machine_id}/{suffix}."""
    ctx = _make_context("local", "bot")

    def handler(q):
        pass

    ctx.queryable("command", handler)
    ctx.session.declare_queryable.assert_called_once_with("bubbaloop/local/bot/command", handler)


def test_queryable_raw_uses_literal_key_expr():
    """queryable_raw() declares at the literal key expression provided."""
    ctx = _make_context("local", "bot")

    def handler(q):
        pass

    ctx.queryable_raw("bubbaloop/**/schema", handler)
    ctx.session.declare_queryable.assert_called_once_with("bubbaloop/**/schema", handler)


def test_queryable_returns_zenoh_queryable():
    """queryable() returns whatever session.declare_queryable returns."""
    ctx = _make_context("local", "bot")
    mock_qbl = MagicMock()
    ctx.session.declare_queryable.return_value = mock_qbl
    result = ctx.queryable("command", lambda q: None)
    assert result is mock_qbl


# ---------------------------------------------------------------------------
# NodeContext.queryable_async() and queryable_raw_async()
# ---------------------------------------------------------------------------


def test_queryable_async_uses_topic_prefix():
    """queryable_async() declares at topic(suffix)."""
    ctx = _make_context("local", "bot")

    def handler(q):
        pass

    qbl = ctx.queryable_async("command", handler)
    try:
        called_topic = ctx.session.declare_queryable.call_args[0][0]
        assert called_topic == "bubbaloop/local/bot/command"
    finally:
        qbl.undeclare()


def test_queryable_async_wraps_handler_in_executor():
    """queryable_async() wraps handler so Zenoh thread is freed."""
    import threading

    ctx = _make_context("local", "bot")
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

    qbl = ctx.queryable_async("command", slow_handler)

    try:
        fake_query = MagicMock()
        captured_wrapper[0](fake_query)  # Zenoh calls the wrapper

        assert event.wait(timeout=2.0), "handler not called within 2s"
        assert received == [fake_query]
    finally:
        qbl.undeclare()


def test_queryable_async_returns_async_queryable():
    """queryable_async() returns AsyncQueryable (not a bare zenoh.Queryable)."""
    from bubbaloop_sdk.subscriber import AsyncQueryable

    ctx = _make_context("local", "bot")
    qbl = ctx.queryable_async("command", lambda q: None)
    try:
        assert isinstance(qbl, AsyncQueryable)
    finally:
        qbl.undeclare()


def test_queryable_raw_async_uses_literal_key_expr():
    """queryable_raw_async() declares at the literal key expression."""
    ctx = _make_context("local", "bot")
    qbl = ctx.queryable_raw_async("bubbaloop/**/schema", lambda q: None)
    try:
        called_topic = ctx.session.declare_queryable.call_args[0][0]
        assert called_topic == "bubbaloop/**/schema"
    finally:
        qbl.undeclare()


def test_queryable_raw_async_wraps_handler_in_executor():
    """queryable_raw_async() wraps handler in thread pool."""
    import threading

    ctx = _make_context("local", "bot")
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

    qbl = ctx.queryable_raw_async("bubbaloop/**/schema", handler)

    try:
        fake_query = MagicMock()
        captured_wrapper[0](fake_query)

        assert event.wait(timeout=2.0), "handler not called within 2s"
        assert received == [fake_query]
    finally:
        qbl.undeclare()


def test_async_queryable_undeclare():
    """AsyncQueryable.undeclare() undeclares queryable then shuts executor."""
    from bubbaloop_sdk.subscriber import AsyncQueryable

    mock_session = MagicMock()
    mock_qbl = MagicMock()
    mock_session.declare_queryable.return_value = mock_qbl
    aq = AsyncQueryable(mock_session, "test/topic", lambda q: None)
    aq.undeclare()
    mock_qbl.undeclare.assert_called_once()


# ---------------------------------------------------------------------------
# NodeContext.subscriber_callback()
# ---------------------------------------------------------------------------


def test_subscriber_callback_uses_topic_prefix():
    """subscriber_callback() declares at topic(suffix)."""
    ctx = _make_context("local", "bot")
    ctx.subscriber_callback("sensor/data", lambda msg: None)
    called_topic = ctx.session.declare_subscriber.call_args[0][0]
    assert called_topic == "bubbaloop/local/bot/sensor/data"


def test_subscriber_raw_callback_uses_literal_key_expr():
    """subscriber_raw_callback() declares at the literal key expression."""
    ctx = _make_context("local", "bot")
    ctx.subscriber_raw_callback("bubbaloop/**/health", lambda s: None)
    called_topic = ctx.session.declare_subscriber.call_args[0][0]
    assert called_topic == "bubbaloop/**/health"


def test_subscriber_callback_async_uses_topic_prefix():
    """subscriber_callback_async() declares at topic(suffix)."""
    ctx = _make_context("local", "bot")
    sub = ctx.subscriber_callback_async("sensor/data", lambda msg: None)
    try:
        called_topic = ctx.session.declare_subscriber.call_args[0][0]
        assert called_topic == "bubbaloop/local/bot/sensor/data"
    finally:
        sub.undeclare()


def test_subscriber_raw_callback_async_uses_literal_key_expr():
    """subscriber_raw_callback_async() declares at literal key expression."""
    ctx = _make_context("local", "bot")
    sub = ctx.subscriber_raw_callback_async("bubbaloop/**/health", lambda s: None)
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
    ctx = _make_context("local", "bot")
    ctx.publisher_json("weather/current")
    called_topic = ctx.session.declare_publisher.call_args[0][0]
    assert called_topic == "bubbaloop/local/bot/weather/current"


def test_publisher_proto_uses_topic_prefix():
    """publisher_proto() declares at topic(suffix)."""
    ctx = _make_context("local", "bot")
    fake_class = MagicMock()
    fake_class.DESCRIPTOR.full_name = "my.SensorData"
    ctx.publisher_proto("sensor/data", fake_class)
    called_topic = ctx.session.declare_publisher.call_args[0][0]
    assert called_topic == "bubbaloop/local/bot/sensor/data"


# ---------------------------------------------------------------------------
# NodeContext.subscriber() / subscriber_raw() via context
# ---------------------------------------------------------------------------


def test_subscriber_uses_topic_prefix():
    """subscriber() declares at topic(suffix)."""
    ctx = _make_context("local", "bot")
    ctx.subscriber("sensor/data")
    called_topic = ctx.session.declare_subscriber.call_args[0][0]
    assert called_topic == "bubbaloop/local/bot/sensor/data"


def test_subscriber_raw_uses_literal_key_expr():
    """subscriber_raw() declares at the literal key expression."""
    ctx = _make_context("local", "bot")
    ctx.subscriber_raw("bubbaloop/**/health")
    called_topic = ctx.session.declare_subscriber.call_args[0][0]
    assert called_topic == "bubbaloop/**/health"


# ---------------------------------------------------------------------------
# NodeContext.close() and context manager
# ---------------------------------------------------------------------------


def test_close_calls_session_close():
    """close() calls session.close()."""
    ctx = _make_context("local", "bot")
    ctx.close()
    ctx.session.close.assert_called_once()


def test_context_manager_calls_close():
    """__exit__ calls close() so the session is always cleaned up."""
    ctx = _make_context("local", "bot")
    with ctx:
        pass
    ctx.session.close.assert_called_once()


# ---------------------------------------------------------------------------
# NodeContext.connect() — env var resolution
# ---------------------------------------------------------------------------


def test_connect_reads_scope_from_env(monkeypatch):
    """BUBBALOOP_SCOPE env var sets ctx.scope."""
    import zenoh

    monkeypatch.setenv("BUBBALOOP_SCOPE", "prod")
    monkeypatch.delenv("BUBBALOOP_MACHINE_ID", raising=False)
    monkeypatch.delenv("BUBBALOOP_ZENOH_ENDPOINT", raising=False)
    monkeypatch.setattr(zenoh, "open", lambda cfg: MagicMock())
    monkeypatch.setattr(zenoh, "Config", MagicMock)
    from bubbaloop_sdk.context import NodeContext

    ctx = NodeContext.connect()
    assert ctx.scope == "prod"


def test_connect_reads_machine_id_from_env(monkeypatch):
    """BUBBALOOP_MACHINE_ID env var sets ctx.machine_id."""
    import zenoh

    monkeypatch.setenv("BUBBALOOP_MACHINE_ID", "jetson_orin")
    monkeypatch.delenv("BUBBALOOP_SCOPE", raising=False)
    monkeypatch.delenv("BUBBALOOP_ZENOH_ENDPOINT", raising=False)
    monkeypatch.setattr(zenoh, "open", lambda cfg: MagicMock())
    monkeypatch.setattr(zenoh, "Config", MagicMock)
    from bubbaloop_sdk.context import NodeContext

    ctx = NodeContext.connect()
    assert ctx.machine_id == "jetson_orin"


def test_connect_defaults_scope_to_local(monkeypatch):
    """scope defaults to 'local' when env var is absent."""
    import zenoh

    monkeypatch.delenv("BUBBALOOP_SCOPE", raising=False)
    monkeypatch.delenv("BUBBALOOP_MACHINE_ID", raising=False)
    monkeypatch.delenv("BUBBALOOP_ZENOH_ENDPOINT", raising=False)
    monkeypatch.setattr(zenoh, "open", lambda cfg: MagicMock())
    monkeypatch.setattr(zenoh, "Config", MagicMock)
    from bubbaloop_sdk.context import NodeContext

    ctx = NodeContext.connect()
    assert ctx.scope == "local"


def test_connect_instance_name_override(monkeypatch):
    """instance_name kwarg overrides hostname fallback."""
    import zenoh

    monkeypatch.delenv("BUBBALOOP_MACHINE_ID", raising=False)
    monkeypatch.delenv("BUBBALOOP_SCOPE", raising=False)
    monkeypatch.delenv("BUBBALOOP_ZENOH_ENDPOINT", raising=False)
    monkeypatch.setattr(zenoh, "open", lambda cfg: MagicMock())
    monkeypatch.setattr(zenoh, "Config", MagicMock)
    from bubbaloop_sdk.context import NodeContext

    ctx = NodeContext.connect(instance_name="tapo_entrance")
    assert ctx.instance_name == "tapo_entrance"


# ---------------------------------------------------------------------------
# TypedSubscriber / RawSubscriber — undeclare() and iteration
# ---------------------------------------------------------------------------


def test_typed_subscriber_undeclare():
    """undeclare() calls undeclare on the underlying zenoh subscriber."""
    from bubbaloop_sdk.subscriber import TypedSubscriber

    mock_session = MagicMock()
    mock_sub = MagicMock()
    mock_session.declare_subscriber.return_value = mock_sub
    sub = TypedSubscriber(mock_session, "test/topic")
    sub.undeclare()
    mock_sub.undeclare.assert_called_once()


def test_raw_subscriber_undeclare():
    """undeclare() calls undeclare on the underlying zenoh subscriber."""
    from bubbaloop_sdk.subscriber import RawSubscriber

    mock_session = MagicMock()
    mock_sub = MagicMock()
    mock_session.declare_subscriber.return_value = mock_sub
    sub = RawSubscriber(mock_session, "test/topic")
    sub.undeclare()
    mock_sub.undeclare.assert_called_once()


def test_typed_subscriber_iteration():
    """Iterating over TypedSubscriber yields decoded messages."""
    from bubbaloop_sdk.subscriber import TypedSubscriber

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(topic, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare
    sub = TypedSubscriber(mock_session, "test/topic")

    # Feed two samples then stop iteration by checking queue empty
    for payload in [b"\x01", b"\x02"]:
        fake_sample = MagicMock()
        fake_sample.payload.to_bytes.return_value = payload
        captured_handler[0](fake_sample)

    results = []
    for msg in sub:
        results.append(msg)
        if len(results) == 2:
            break

    assert results == [b"\x01", b"\x02"]


def test_typed_subscriber_recv_returns_none_after_undeclare():
    """recv() returns None immediately on all calls after undeclare()."""
    from bubbaloop_sdk.subscriber import TypedSubscriber

    mock_session = MagicMock()
    mock_session.declare_subscriber.return_value = MagicMock()
    sub = TypedSubscriber(mock_session, "test/topic")
    sub.undeclare()
    # First call consumes the sentinel; second must not block.
    assert sub.recv(timeout=1.0) is None
    assert sub.recv(timeout=1.0) is None


def test_raw_subscriber_recv_returns_none_after_undeclare():
    """recv() returns None immediately on all calls after undeclare()."""
    from bubbaloop_sdk.subscriber import RawSubscriber

    mock_session = MagicMock()
    mock_session.declare_subscriber.return_value = MagicMock()
    sub = RawSubscriber(mock_session, "test/topic")
    sub.undeclare()
    assert sub.recv(timeout=1.0) is None
    assert sub.recv(timeout=1.0) is None


def test_typed_subscriber_drops_samples_after_undeclare():
    """Samples arriving after undeclare() are not enqueued."""
    from bubbaloop_sdk.subscriber import TypedSubscriber

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(topic, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare
    sub = TypedSubscriber(mock_session, "test/topic")
    sub.undeclare()

    fake_sample = MagicMock()
    fake_sample.payload.to_bytes.return_value = b"\xff"
    captured_handler[0](fake_sample)  # arrives after undeclare

    assert sub.recv(timeout=0.1) is None  # no message — only closed state


def test_raw_subscriber_drops_samples_after_undeclare():
    """Samples arriving after undeclare() are not enqueued."""
    from bubbaloop_sdk.subscriber import RawSubscriber

    mock_session = MagicMock()
    captured_handler = []

    def fake_declare(key_expr, handler):
        captured_handler.append(handler)
        return MagicMock()

    mock_session.declare_subscriber.side_effect = fake_declare
    sub = RawSubscriber(mock_session, "test/topic")
    sub.undeclare()

    fake_sample = MagicMock()
    captured_handler[0](fake_sample)  # arrives after undeclare

    assert sub.recv(timeout=0.1) is None


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
