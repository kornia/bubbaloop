"""Builders for canonical bubbaloop wire-format envelopes (CBOR + JSON).

See `docs/concepts/wire-format.md` upstream for the schema.
"""

from __future__ import annotations

import json
import time
from collections.abc import Mapping
from typing import Any

import cbor2


def make_envelope(
    schema_uri: str,
    body: Any,
    *,
    source_instance: str = "test_instance",
    ts_ns: int | None = None,
    monotonic_seq: int = 0,
) -> bytes:
    """Build a CBOR-encoded canonical envelope.

    `body` is encoded as-is (any CBOR-encodable value). The header carries
    transport metadata (schema_uri, source_instance, ts_ns, monotonic_seq)
    per the wire-format spec.
    """
    return cbor2.dumps(
        {
            "header": {
                "schema_uri": schema_uri,
                "source_instance": source_instance,
                "ts_ns": ts_ns if ts_ns is not None else time.time_ns(),
                "monotonic_seq": monotonic_seq,
            },
            "body": body,
        }
    )


def make_command(command: str, **params: Any) -> bytes:
    """Build a flat-format JSON command envelope (bubbaloop ≥ PR #80).

    Use this to drive a node's `command` queryable in tests.
    """
    return json.dumps({"command": command, **params}).encode()


def make_command_nested(command: str, params: Mapping[str, Any] | None = None) -> bytes:
    """Build a nested-format JSON command envelope (older daemons).

    Tests for nodes that claim dual-envelope support should exercise both
    `make_command` and `make_command_nested`.
    """
    return json.dumps({"command": command, "params": dict(params or {})}).encode()
