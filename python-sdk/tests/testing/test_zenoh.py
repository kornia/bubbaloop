"""Tests for the in-memory FakeZenohSession."""

from __future__ import annotations

import pytest

from bubbaloop_sdk.testing import FakeZenohSession, key_matches

# ── key_matches ────────────────────────────────────────────────────


@pytest.mark.parametrize(
    ("pattern", "key", "expected"),
    [
        ("a/b/c", "a/b/c", True),
        ("a/b/c", "a/b/d", False),
        ("a/*/c", "a/b/c", True),
        ("a/*/c", "a/b/d/c", False),  # * is single-segment
        ("a/**", "a", True),  # ** matches zero segments too
        ("a/**", "a/b/c/d", True),
        ("a/**/d", "a/b/c/d", True),
        ("**/x", "a/b/x", True),
        ("**/x", "x", True),
        ("bubbaloop/global/*/cam/**", "bubbaloop/global/m1/cam/compressed", True),
        ("bubbaloop/global/*/cam/**", "bubbaloop/global/m1/m2/cam/compressed", False),
        # Dot in topic name must not be treated as regex metacharacter
        ("foo.bar", "foo.bar", True),
        ("foo.bar", "fooxbar", False),
    ],
)
def test_key_matches(pattern: str, key: str, expected: bool):
    assert key_matches(pattern, key) is expected


# ── publish/subscribe ──────────────────────────────────────────────


def test_publish_routes_to_matching_subscriber():
    s = FakeZenohSession()
    seen: list[bytes] = []
    s.declare_subscriber("a/b", lambda sample: seen.append(sample.payload))

    assert s.publish("a/b", b"hello") == 1
    assert seen == [b"hello"]


def test_publish_skips_non_matching_subscribers():
    s = FakeZenohSession()
    a_seen: list[bytes] = []
    b_seen: list[bytes] = []
    s.declare_subscriber("topic/a", lambda sample: a_seen.append(sample.payload))
    s.declare_subscriber("topic/b", lambda sample: b_seen.append(sample.payload))

    s.publish("topic/a", b"x")

    assert a_seen == [b"x"]
    assert b_seen == []


def test_publish_fans_out_to_all_matching_subscribers():
    s = FakeZenohSession()
    seen_a: list[bytes] = []
    seen_b: list[bytes] = []
    s.declare_subscriber("ns/**", lambda sample: seen_a.append(sample.payload))
    s.declare_subscriber("ns/x", lambda sample: seen_b.append(sample.payload))

    assert s.publish("ns/x", b"data") == 2
    assert seen_a == [b"data"]
    assert seen_b == [b"data"]


def test_subscriber_undeclare_stops_delivery():
    s = FakeZenohSession()
    seen: list[bytes] = []
    h = s.declare_subscriber("a/b", lambda sample: seen.append(sample.payload))

    s.publish("a/b", b"first")
    h.undeclare()
    s.publish("a/b", b"second")

    assert seen == [b"first"]
    assert h.undeclared is True


def test_sample_carries_encoding():
    s = FakeZenohSession()
    captured: list[str] = []
    s.declare_subscriber("a", lambda sample: captured.append(sample.encoding))

    s.publish("a", b"x", encoding="application/json")
    assert captured == ["application/json"]


# ── queryable ──────────────────────────────────────────────────────


def test_queryable_receives_query_and_replies():
    s = FakeZenohSession()

    def handler(query):
        assert str(query.key_expr) == "rec/command"
        assert query.payload == b'{"cmd":"start"}'
        query.reply(query.key_expr, b'{"status":"started"}')

    s.declare_queryable("rec/command", handler)

    replies = s.query("rec/command", b'{"cmd":"start"}')
    assert replies == [b'{"status":"started"}']


def test_queryable_multiple_replies():
    s = FakeZenohSession()

    def handler(query):
        query.reply(query.key_expr, b"chunk-1")
        query.reply(query.key_expr, b"chunk-2")

    s.declare_queryable("multi/reply", handler)
    assert s.query("multi/reply") == [b"chunk-1", b"chunk-2"]


def test_query_to_unmatched_key_returns_empty():
    s = FakeZenohSession()
    s.declare_queryable("a/b", lambda q: q.reply(q.key_expr, b"x"))
    assert s.query("c/d") == []


# ── introspection ──────────────────────────────────────────────────


def test_subscriber_patterns_after_undeclare():
    s = FakeZenohSession()
    h1 = s.declare_subscriber("a", lambda _: None)
    s.declare_subscriber("b", lambda _: None)
    h1.undeclare()
    assert sorted(s.subscriber_patterns()) == ["b"]


def test_close_undeclares_everything():
    s = FakeZenohSession()
    s.declare_subscriber("a", lambda _: None)
    s.declare_queryable("b", lambda _: None)
    s.close()
    assert s.subscriber_patterns() == []
    assert s.queryable_patterns() == []
