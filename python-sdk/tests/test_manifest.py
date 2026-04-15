"""Tests for the dataflow manifest queryable layer."""

import threading
from unittest.mock import MagicMock

import cbor2

from bubbaloop_sdk.manifest import (
    MANIFEST_SCHEMA_VERSION,
    build_manifest,
    manifest_topic,
    normalize_role,
)


def _ctx(machine_id="bot", instance_name="emb"):
    from bubbaloop_sdk.context import NodeContext
    ctx = object.__new__(NodeContext)
    ctx.session = MagicMock()
    ctx.machine_id = machine_id
    ctx.instance_name = instance_name
    ctx._shutdown = threading.Event()
    return ctx


def test_normalize_role_known():
    assert normalize_role("source") == "source"
    assert normalize_role("Processor") == "processor"
    assert normalize_role("SINK") == "sink"


def test_normalize_role_unknown():
    assert normalize_role(None) == "unknown"
    assert normalize_role("") == "unknown"
    assert normalize_role("weirdo") == "unknown"


def test_manifest_topic_format():
    assert manifest_topic("bot", "emb") == "bubbaloop/global/bot/emb/manifest"


def test_build_manifest_uses_liveness_snapshots():
    ctx = _ctx()
    ctx._declare_output("bubbaloop/global/bot/emb/embeddings")
    ctx._declare_input("bubbaloop/global/bot/upstream/raw")
    # Mark the output as ever_fired — simulates a put() call.
    ctx._mark_output_fired("emb/embeddings")
    m = build_manifest(ctx, role="processor", started_at_ns=42)

    assert m["instance_name"] == "emb"
    assert m["machine_id"] == "bot"
    assert m["role"] == "processor"
    out = m["outputs"]
    assert len(out) == 1
    assert out[0]["topic"] == "emb/embeddings"
    assert out[0]["ever_fired"] is True
    assert out[0]["still_live"] is True
    assert isinstance(out[0]["declared_at_ns"], int)
    assert out[0]["declared_at_ns"] > 0
    inp = m["inputs"]
    assert len(inp) == 1
    assert inp[0]["topic"] == "upstream/raw"
    assert inp[0]["ever_fired"] is False
    assert inp[0]["still_live"] is True
    assert m["schema_version"] == MANIFEST_SCHEMA_VERSION
    assert m["started_at_ns"] == 42
    assert m["node_kind"] == "python"


def test_manifest_roundtrips_via_cbor():
    ctx = _ctx()
    ctx._declare_output("bubbaloop/global/bot/emb/out")
    ctx._mark_output_fired("emb/out")
    payload = cbor2.dumps(build_manifest(ctx, role="source", started_at_ns=7))
    decoded = cbor2.loads(payload)
    assert len(decoded["outputs"]) == 1
    assert decoded["outputs"][0]["topic"] == "emb/out"
    assert decoded["outputs"][0]["ever_fired"] is True
    assert decoded["role"] == "source"
    assert decoded["schema_version"] == MANIFEST_SCHEMA_VERSION


def test_record_dedupes_repeats():
    ctx = _ctx()
    ctx._declare_output("bubbaloop/global/bot/emb/out")
    ctx._declare_output("bubbaloop/global/bot/emb/out")
    ctx._declare_input("bubbaloop/local/bot/raw/frames")
    ctx._declare_input("bubbaloop/local/bot/raw/frames")
    assert [e["topic"] for e in ctx.outputs_snapshot()] == ["emb/out"]
    assert [e["topic"] for e in ctx.inputs_snapshot()] == ["raw/frames"]


def test_record_ignores_foreign_machine_keys():
    ctx = _ctx()
    ctx._declare_output("bubbaloop/global/other_host/emb/out")
    assert ctx.outputs_snapshot() == []


def test_declared_but_never_fired_shows_in_manifest():
    """A publisher that declared but never called put() must still surface
    as ``ever_fired=false`` (never hidden) so the dataflow tool can flag
    conditional outputs behind unfired branches."""
    ctx = _ctx()
    ctx._declare_output("bubbaloop/global/bot/emb/rare")
    entries = ctx.outputs_snapshot()
    assert len(entries) == 1
    assert entries[0]["ever_fired"] is False
    assert entries[0]["still_live"] is True


def test_undeclare_flips_still_live_without_removing():
    ctx = _ctx()
    ctx._declare_output("bubbaloop/global/bot/emb/x")
    ctx._mark_output_undeclared("emb/x")
    entries = ctx.outputs_snapshot()
    assert len(entries) == 1
    assert entries[0]["still_live"] is False
