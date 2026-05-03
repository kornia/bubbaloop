"""bubbaloop_sdk.testing — fakes for testing nodes without zenohd.

    from bubbaloop_sdk.testing import (
        FakeContext,
        FakeZenohSession,
        make_envelope,
        make_command,
    )

See `bubbaloop_sdk/testing/zenoh.py` for the supported pattern syntax
(`*` single-segment, `**` multi-segment).
"""

from .context import FakeContext
from .envelope import make_command, make_command_nested, make_envelope
from .zenoh import FakeKeyExpr, FakeQuery, FakeSample, FakeZenohSession, key_matches

__all__ = [
    "FakeContext",
    "FakeKeyExpr",
    "FakeQuery",
    "FakeSample",
    "FakeZenohSession",
    "key_matches",
    "make_command",
    "make_command_nested",
    "make_envelope",
]
