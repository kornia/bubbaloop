"""Declared publishers for JSON, CBOR, and raw messages."""

import json
import time
from typing import Any, Callable, Optional

import cbor2
import zenoh

# APPLICATION_CBOR encoding id=8
_CBOR_ENCODING = zenoh.Encoding.APPLICATION_CBOR


def _wrap_envelope(body, source_instance: str, schema_uri: str, seq: int) -> dict:
    """Build a `{header, body}` provenance envelope.

    Header fields are filled by the SDK; ``schema_uri`` is the only caller-provided
    field (defaults to a synthesized ``bubbaloop://...`` URI). The envelope makes
    every wire sample self-describing — an LLM that decodes one message learns
    *what* it is (``schema_uri``) and *where it came from* (``source_instance``,
    ``monotonic_seq``, ``ts_ns``) with zero side-channel calls.
    """
    return {
        "header": {
            "schema_uri": schema_uri,
            "source_instance": source_instance,
            "monotonic_seq": seq,
            "ts_ns": time.time_ns(),
        },
        "body": body,
    }


class _BasePublisher:
    """Shared cleanup for all publisher types."""

    def __init__(self, declared_publisher: zenoh.Publisher):
        self._pub = declared_publisher
        # Optional callback invoked the first time a payload is actually
        # published. Used by NodeContext to flip the manifest `ever_fired`
        # bit so the dataflow tool can distinguish declared-but-idle
        # publishers from actively firing ones.
        self._on_first_fire: Optional[Callable[[], None]] = None
        # Optional callback invoked when the publisher is undeclared; lets
        # NodeContext flip `still_live=False` for manifest history.
        self._on_undeclare: Optional[Callable[[], None]] = None

    def _fire(self) -> None:
        cb = self._on_first_fire
        if cb is not None:
            self._on_first_fire = None
            try:
                cb()
            except Exception:  # pragma: no cover — defensive
                pass

    def undeclare(self) -> None:
        cb = self._on_undeclare
        self._on_undeclare = None
        self._pub.undeclare()
        if cb is not None:
            try:
                cb()
            except Exception:  # pragma: no cover — defensive
                pass


class JsonPublisher(_BasePublisher):
    """Declared publisher that sets APPLICATION_JSON encoding on every sample.

    Wraps every payload in the SDK's ``{header, body}`` provenance envelope —
    the same shape :class:`CborPublisher` uses — so lineage information
    (``schema_uri``, ``source_instance``, ``monotonic_seq``, ``ts_ns``) is
    present on JSON edges too. Pre-encoded ``bytes`` / ``str`` bypass the
    envelope (treated as already-final wire bytes).
    """

    def __init__(
        self,
        declared_publisher: zenoh.Publisher,
        source_instance: str = "",
        schema_uri: str = "",
    ):
        super().__init__(declared_publisher)
        self._source_instance = source_instance
        self._schema_uri = schema_uri
        self._seq = 0

    @classmethod
    def _declare(
        cls,
        session: zenoh.Session,
        topic: str,
        source_instance: str = "",
        schema_uri: str = "",
    ) -> "JsonPublisher":
        pub = session.declare_publisher(topic, encoding=zenoh.Encoding.APPLICATION_JSON)
        return cls(pub, source_instance=source_instance, schema_uri=schema_uri)

    def put(self, value) -> None:
        """Publish a JSON-serializable value wrapped in a provenance envelope.

        ``bytes``/``bytearray``/``str`` are passed through unmodified — the
        caller is assumed to have pre-built the wire payload.
        """
        if isinstance(value, (bytes, bytearray)):
            self._pub.put(bytes(value))
            self._fire()
            return
        if isinstance(value, str):
            self._pub.put(value.encode())
            self._fire()
            return
        envelope = _wrap_envelope(
            value,
            source_instance=self._source_instance,
            schema_uri=self._schema_uri,
            seq=self._seq,
        )
        self._seq += 1
        self._pub.put(json.dumps(envelope).encode())
        self._fire()


class CborPublisher(_BasePublisher):
    """Declared publisher that sets APPLICATION_CBOR encoding on every sample.

    Wraps every payload in a ``{header, body}`` provenance envelope before
    serializing. The header carries ``schema_uri``, ``source_instance``,
    ``monotonic_seq`` (per-publisher counter, starts at 0), and ``ts_ns``
    (wall-clock nanoseconds). Pre-encoded ``bytes``/``bytearray`` payloads
    bypass the envelope (they're treated as already-final wire bytes).

    When ``local=True`` is passed to :meth:`_declare`, the publisher uses
    ``congestion_control=CongestionControl.BLOCK`` so SHM frames are never
    silently dropped when the subscriber is slow. The topic key must already
    live under ``bubbaloop/local/...`` (callers are responsible for that —
    see :meth:`NodeContext.publisher_cbor`).

    Usage::

        pub = ctx.publisher_cbor("sensor/data", schema_uri="bubbaloop://sensor/v1")
        pub.put({"temperature": 22.5, "humidity": 60})
        # On wire: {"header": {...}, "body": {"temperature": 22.5, ...}}

        # SHM variant — BLOCK congestion control, local key space:
        pub = ctx.publisher_cbor("rgbd", local=True, schema_uri="bubbaloop://rgbd/v1")
        pub.put({"width": 1280, "height": 720, "data": frame_bytes})
    """

    def __init__(
        self,
        declared_publisher: zenoh.Publisher,
        source_instance: str = "",
        schema_uri: str = "",
    ):
        super().__init__(declared_publisher)
        self._source_instance = source_instance
        self._schema_uri = schema_uri
        self._seq = 0

    @classmethod
    def _declare(
        cls,
        session: zenoh.Session,
        topic: str,
        source_instance: str = "",
        schema_uri: str = "",
        local: bool = False,
    ) -> "CborPublisher":
        kwargs: dict[str, Any] = {"encoding": _CBOR_ENCODING}
        if local:
            kwargs["congestion_control"] = zenoh.CongestionControl.BLOCK
        pub = session.declare_publisher(topic, **kwargs)
        return cls(pub, source_instance=source_instance, schema_uri=schema_uri)

    def put(self, value) -> None:
        """Publish a CBOR-encoded value wrapped in a provenance envelope.

        Pre-encoded ``bytes``/``bytearray`` are passed through unmodified — the
        caller is assumed to have already built the wire payload.
        """
        if isinstance(value, (bytes, bytearray)):
            self._pub.put(bytes(value))
            self._fire()
            return
        envelope = _wrap_envelope(
            value,
            source_instance=self._source_instance,
            schema_uri=self._schema_uri,
            seq=self._seq,
        )
        self._seq += 1
        self._pub.put(cbor2.dumps(envelope))
        self._fire()


class RawPublisher(_BasePublisher):
    """Declared publisher for raw byte payloads with no encoding overhead.

    When ``local=True``, uses ``congestion_control=Block`` — the publisher waits for
    the subscriber to release the SHM buffer instead of silently dropping frames.
    Topic is ``local/{machine_id}/suffix`` (never crosses the WebSocket bridge).

    When ``local=False`` (default), uses standard drop-on-congestion and publishes
    to the global ``bubbaloop/**`` topic space.
    """

    @classmethod
    def _declare(cls, session: zenoh.Session, topic: str, local: bool = False) -> "RawPublisher":
        kwargs: dict[str, Any] = {}
        if local:
            kwargs["congestion_control"] = zenoh.CongestionControl.BLOCK
        pub = session.declare_publisher(topic, **kwargs)
        return cls(pub)

    def put(self, data: bytes | bytearray) -> None:
        """Publish raw bytes."""
        self._pub.put(bytes(data))
        self._fire()
