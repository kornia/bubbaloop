"""In-memory fake Zenoh session for unit tests.

Mirrors enough of the `zenoh.Session` surface that bubbaloop nodes which
talk raw Zenoh (declare_subscriber / declare_queryable / get) can be
exercised in-process, with no router, deterministically, in milliseconds.

Pattern matching uses the standard Zenoh shorthand:
  *   matches a single key segment (no slashes)
  **  matches any number of segments (including zero)

Tests drive the fake by calling `session.publish(key, payload, encoding)`
to deliver a sample to all matching subscribers, or
`session.query(key, payload)` to dispatch to a registered queryable and
collect the reply.
"""

from __future__ import annotations

import fnmatch
import threading
from collections.abc import Callable, Sequence
from dataclasses import dataclass, field

# ── Pattern matching ───────────────────────────────────────────────


def _match_segments(pat: Sequence[str], key: Sequence[str]) -> bool:
    """Recursive segment-level matcher.

    `**` consumes zero or more key segments; `*` consumes exactly one;
    everything else uses fnmatch (so `cam_*` matches `cam_foo`). Done
    recursively because `**` is non-greedy by nature — multiple `**`s
    in one pattern need backtracking.
    """
    if not pat:
        return not key
    head, rest = pat[0], pat[1:]

    if head == "**":
        # Zero or more segments: try matching the rest at every position.
        return any(_match_segments(rest, key[i:]) for i in range(len(key) + 1))

    if not key:
        return False

    if head == "*":
        return _match_segments(rest, key[1:])

    return fnmatch.fnmatchcase(key[0], head) and _match_segments(rest, key[1:])


def key_matches(pattern: str, key: str) -> bool:
    """Return True if `key` matches a Zenoh-style `pattern`.

    `*`  matches a single key segment (no slashes).
    `**` matches zero or more segments.
    """
    return _match_segments(pattern.split("/"), key.split("/"))


# ── Sample / Query / Encoding ──────────────────────────────────────


class FakeKeyExpr:
    """Stand-in for `zenoh.KeyExpr` — the production API stringifies it."""

    __slots__ = ("_s",)

    def __init__(self, s: str):
        self._s = s

    def __str__(self) -> str:
        return self._s

    def __repr__(self) -> str:
        return f"FakeKeyExpr({self._s!r})"


@dataclass
class FakeSample:
    """Mirrors the fields production node code reads off `zenoh.Sample`."""

    key_expr: FakeKeyExpr
    payload: bytes
    encoding: str = "application/cbor"


@dataclass
class FakeQuery:
    """Mirrors `zenoh.Query`. `reply()` records bytes for the test to inspect."""

    key_expr: FakeKeyExpr
    payload: bytes | None
    _replies: list[tuple[FakeKeyExpr, bytes]] = field(default_factory=list)

    def reply(self, key_expr, payload) -> None:
        ke = key_expr if isinstance(key_expr, FakeKeyExpr) else FakeKeyExpr(str(key_expr))
        self._replies.append((ke, bytes(payload)))


# ── Subscriber / Queryable handles ─────────────────────────────────


class _Handle:
    """Common base for declared subscriber/queryable handles. Tracks
    undeclare so tests can assert that nodes clean up."""

    __slots__ = ("_session", "_pattern", "_callback", "_kind", "undeclared")

    def __init__(self, session: FakeZenohSession, pattern: str, callback: Callable, kind: str):
        self._session = session
        self._pattern = pattern
        self._callback = callback
        self._kind = kind
        self.undeclared = False

    def undeclare(self) -> None:
        if self.undeclared:
            return
        self.undeclared = True
        self._session._remove(self)


# ── Session ────────────────────────────────────────────────────────


class FakeZenohSession:
    """In-memory pub/sub + queryable session for tests.

    Thread-safe: subscribers can be invoked from the test thread while
    nodes hold the session. Patterned after Zenoh's own thread model so
    tests catch the same race conditions you'd hit in production.
    """

    def __init__(self) -> None:
        self._lock = threading.RLock()
        self._subscribers: list[_Handle] = []
        self._queryables: list[_Handle] = []

    # ── Production-side API (called by node code) ────────────────

    def declare_subscriber(self, key_expr: str, callback: Callable) -> _Handle:
        h = _Handle(self, key_expr, callback, "subscriber")
        with self._lock:
            self._subscribers.append(h)
        return h

    def declare_queryable(self, key_expr: str, callback: Callable) -> _Handle:
        h = _Handle(self, key_expr, callback, "queryable")
        with self._lock:
            self._queryables.append(h)
        return h

    # ── Test-side API (called by tests to drive the session) ─────

    def publish(
        self,
        key: str,
        payload: bytes,
        encoding: str = "application/cbor",
    ) -> int:
        """Deliver `payload` to every subscriber whose pattern matches `key`.
        Returns the number of subscribers that fired."""
        sample = FakeSample(FakeKeyExpr(key), bytes(payload), encoding)
        # Snapshot under lock so subscriber callbacks (which may declare
        # new subscribers themselves) don't deadlock.
        with self._lock:
            matched = [h for h in self._subscribers if key_matches(h._pattern, key)]
        for h in matched:
            h._callback(sample)
        return len(matched)

    def query(self, key: str, payload: bytes = b"") -> list[bytes]:
        """Dispatch a query to every matching queryable and return collected
        reply payloads in declaration order."""
        with self._lock:
            matched = [h for h in self._queryables if key_matches(h._pattern, key)]
        replies: list[bytes] = []
        for h in matched:
            q = FakeQuery(FakeKeyExpr(key), bytes(payload))
            h._callback(q)
            replies.extend(p for _, p in q._replies)
        return replies

    # ── Introspection ─────────────────────────────────────────────

    def subscriber_patterns(self) -> list[str]:
        with self._lock:
            return [h._pattern for h in self._subscribers if not h.undeclared]

    def queryable_patterns(self) -> list[str]:
        with self._lock:
            return [h._pattern for h in self._queryables if not h.undeclared]

    # ── Internal ──────────────────────────────────────────────────

    def _remove(self, handle: _Handle) -> None:
        with self._lock:
            for bucket in (self._subscribers, self._queryables):
                if handle in bucket:
                    bucket.remove(handle)
                    return

    def close(self) -> None:
        """Match `zenoh.Session.close()` — undeclare everything."""
        with self._lock:
            for h in list(self._subscribers) + list(self._queryables):
                h.undeclared = True
            self._subscribers.clear()
            self._queryables.clear()
