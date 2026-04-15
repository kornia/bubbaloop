"""Blocking Zenoh subscribers."""

import json
import logging
import types
from dataclasses import dataclass
from typing import Any

import cbor2
import zenoh

log = logging.getLogger(__name__)

_CBOR_ENCODING = "application/cbor"
_JSON_ENCODING = "application/json"


@dataclass
class Envelope:
    """Provenance-envelope wrapper returned by :class:`CborSubscriber` when
    the decoded payload has the SDK's ``{header, body}`` shape.

    ``body`` is the user payload (``SimpleNamespace`` for dicts).
    ``header`` is a ``SimpleNamespace`` exposing ``schema_uri``,
    ``source_instance``, ``monotonic_seq``, ``ts_ns``.

    Iterate as ``for env in sub: msg = env.body``. For passthrough safety,
    callers can use ``getattr(env, 'body', env)`` — non-enveloped upstreams
    return the raw decoded value.
    """

    body: Any
    header: Any


def _dict_to_namespace(obj):
    """Recursively convert dicts to SimpleNamespace for attribute access."""
    if isinstance(obj, dict):
        return types.SimpleNamespace(**{k: _dict_to_namespace(v) for k, v in obj.items()})
    if isinstance(obj, list):
        return [_dict_to_namespace(item) for item in obj]
    return obj


def _maybe_unwrap_envelope(decoded):
    """Return an :class:`Envelope` if ``decoded`` is a `{header, body}` dict
    (the SDK provenance shape used on both CBOR and JSON topics), otherwise
    return the value converted via :func:`_dict_to_namespace`.

    Defensive: requires BOTH keys, ``header`` to be a dict, and the dict to
    have exactly those two top-level keys (so we don't accidentally wrap a
    user payload that happens to define its own ``header`` field).
    """
    if (
        isinstance(decoded, dict)
        and set(decoded.keys()) == {"header", "body"}
        and isinstance(decoded["header"], dict)
    ):
        return Envelope(
            body=_dict_to_namespace(decoded["body"]),
            header=_dict_to_namespace(decoded["header"]),
        )
    return _dict_to_namespace(decoded) if isinstance(decoded, dict) else decoded


def _maybe_unwrap_envelope_plain(decoded):
    """Like :func:`_maybe_unwrap_envelope` but leaves non-enveloped payloads
    as raw dicts (no ``SimpleNamespace`` conversion). Used for JSON where the
    long-standing contract is to return a plain ``dict``."""
    if (
        isinstance(decoded, dict)
        and set(decoded.keys()) == {"header", "body"}
        and isinstance(decoded["header"], dict)
    ):
        return Envelope(
            body=_dict_to_namespace(decoded["body"]),
            header=_dict_to_namespace(decoded["header"]),
        )
    return decoded


class _BaseSubscriber:
    """Shared iterator protocol and cleanup for all subscriber types."""

    def __init__(self, session: zenoh.Session, topic: str):
        self._sub = session.declare_subscriber(topic)
        self._undeclared = False

    def recv(self):
        raise NotImplementedError

    def __iter__(self):
        return self

    def __next__(self):
        try:
            return self.recv()
        except Exception as exc:
            raise StopIteration from exc

    def undeclare(self) -> None:
        """Release the underlying Zenoh subscriber. Idempotent.

        Double-close is common during shutdown (explicit call + context-manager
        exit, or test teardown racing with ``__del__``). We guard with a flag so
        a legitimate first-call ``RuntimeError`` from Zenoh still propagates —
        only repeat calls become no-ops.
        """
        if self._undeclared:
            return
        self._undeclared = True
        self._sub.undeclare()


class CborSubscriber(_BaseSubscriber):
    """Blocking subscriber that auto-decodes messages by encoding.

    Supported encodings:

    - ``application/cbor`` → SimpleNamespace (attribute access) for dicts, else native Python
    - ``application/json`` → dict
    - anything else        → raw ``bytes``

    Usage::

        sub = ctx.subscribe("tapo_terrace/raw", local=True)
        for msg in sub:   # CBOR decoded automatically (msg.width, msg.data, etc.)
            tensor = torch.frombuffer(msg.data, dtype=torch.uint8)

        sub = ctx.subscribe("openmeteo/weather")
        for msg in sub:   # dict
            print(msg["temperature"])
    """

    def recv(self):
        """Block until next message and return the decoded object."""
        sample = self._sub.recv()
        encoding = str(sample.encoding)
        payload = bytes(sample.payload)

        if encoding == _CBOR_ENCODING:
            try:
                decoded = cbor2.loads(payload)
                return _maybe_unwrap_envelope(decoded)
            except Exception as exc:
                log.debug("CBOR decode failed: %s", exc)
                return payload

        if encoding == _JSON_ENCODING:
            try:
                decoded = json.loads(payload)
                return _maybe_unwrap_envelope_plain(decoded)
            except Exception as exc:
                log.debug("JSON decode failed: %s", exc)
                return payload

        return payload


class RawSubscriber(_BaseSubscriber):
    """Blocking subscriber that yields raw ``bytes``, counterpart to :class:`RawPublisher`.

    No decoding is applied — the caller owns the byte layout entirely.
    SHM zero-copy delivery is used automatically when both sides have the session
    SHM transport enabled, but the subscriber works over any Zenoh transport.

    Usage::

        sub = ctx.subscribe_raw("camera/raw", local=True)
        for raw_bytes in sub:
            tensor = torch.frombuffer(raw_bytes, dtype=torch.uint8)
    """

    def recv(self) -> bytes:
        """Block until the next frame arrives and return the raw bytes."""
        sample = self._sub.recv()
        return bytes(sample.payload)
