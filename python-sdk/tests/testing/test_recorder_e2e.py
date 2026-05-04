"""End-to-end test of a real bubbaloop node (mcap-recorder) against the
fakes — proves the testing harness can drive a live node's full lifecycle
without zenohd.

Skipped if the recorder isn't on the import path (it lives in a sibling
repo, bubbaloop-nodes-official, and isn't a hard dep of the SDK).
"""

from __future__ import annotations

import json
import sys
import threading
import time
from pathlib import Path

import pytest

from bubbaloop_sdk.testing import (
    FakeContext,
    make_command,
    make_command_nested,
    make_envelope,
)


def _try_import_recorder():
    # python-sdk/tests/testing/test_recorder_e2e.py → ../../../../ → workspace root
    workspace = Path(__file__).resolve().parents[4]
    candidate = workspace / "bubbaloop-nodes-official" / "mcap-recorder"
    if not candidate.exists():
        return None
    sys.path.insert(0, str(candidate))
    try:
        from recorder.node import RecorderNode  # type: ignore[import-not-found]

        return RecorderNode
    except Exception:
        return None


RecorderNode = _try_import_recorder()
pytestmark = pytest.mark.skipif(
    RecorderNode is None,
    reason="bubbaloop-nodes-official/mcap-recorder not on path",
)


def _run_node_in_thread(node) -> threading.Thread:
    t = threading.Thread(target=node.run, daemon=True)
    t.start()
    # Small delay so the queryable is declared before the test fires queries.
    time.sleep(0.05)
    return t


def _query_command(ctx, machine_id, instance, command_bytes):
    key = f"bubbaloop/global/{machine_id}/{instance}/command"
    replies = ctx.session.query(key, command_bytes)
    assert replies, "queryable did not reply"
    return json.loads(replies[0])


def test_get_status_when_idle(tmp_path):
    ctx = FakeContext(machine_id="test", instance_name="rec")
    node = RecorderNode(ctx, {"name": "rec", "output_dir": str(tmp_path)})
    t = _run_node_in_thread(node)
    try:
        reply = _query_command(ctx, "test", "rec", make_command("get_status"))
        assert reply == {"status": "idle"}
    finally:
        ctx.signal_shutdown()
        t.join(timeout=2.0)


def test_start_stop_cycle_writes_one_chunk(tmp_path):
    ctx = FakeContext(machine_id="test", instance_name="rec")
    node = RecorderNode(ctx, {"name": "rec", "output_dir": str(tmp_path)})
    t = _run_node_in_thread(node)
    try:
        start = _query_command(
            ctx,
            "test",
            "rec",
            make_command("start_recording", topic_patterns=["bubbaloop/global/test/cam/**"]),
        )
        assert start["status"] == "started"
        session_id = start["session_id"]

        # Publish some fake camera frames matching the pattern.
        for i in range(5):
            ctx.session.publish(
                "bubbaloop/global/test/cam/compressed",
                make_envelope(
                    "bubbaloop://schemas/sensor/CompressedImage/v1",
                    body={"data": b"fake-h264", "format": "h264"},
                    monotonic_seq=i,
                ),
            )
        # Tiny wait so the writer thread drains the queue.
        time.sleep(0.2)

        status = _query_command(ctx, "test", "rec", make_command("get_status"))
        assert status["status"] == "recording"
        assert status["messages_recorded"] == 5

        stop = _query_command(ctx, "test", "rec", make_command("stop_recording"))
        assert stop["status"] == "stopped"
        assert stop["total_messages"] == 5
        assert stop["session_id"] == session_id

        # Chunk file finalised on disk.
        chunks = list(tmp_path.glob("*.mcap"))
        assert chunks, f"no .mcap chunks in {tmp_path}"
    finally:
        ctx.signal_shutdown()
        t.join(timeout=2.0)


def test_double_start_returns_E_ALREADY_RECORDING(tmp_path):
    ctx = FakeContext(machine_id="test", instance_name="rec")
    node = RecorderNode(ctx, {"name": "rec", "output_dir": str(tmp_path)})
    t = _run_node_in_thread(node)
    try:
        first = _query_command(
            ctx,
            "test",
            "rec",
            make_command("start_recording", topic_patterns=["bubbaloop/global/test/cam/**"]),
        )
        assert first["status"] == "started"

        second = _query_command(
            ctx,
            "test",
            "rec",
            make_command("start_recording", topic_patterns=["bubbaloop/global/test/cam/**"]),
        )
        assert second["status"] == "error"
        assert second["code"] == "E_ALREADY_RECORDING"
    finally:
        ctx.signal_shutdown()
        t.join(timeout=2.0)


def test_unknown_command_returns_E_UNKNOWN_CMD(tmp_path):
    ctx = FakeContext(machine_id="test", instance_name="rec")
    node = RecorderNode(ctx, {"name": "rec", "output_dir": str(tmp_path)})
    t = _run_node_in_thread(node)
    try:
        reply = _query_command(ctx, "test", "rec", make_command("nope"))
        assert reply["status"] == "error"
        assert reply["code"] == "E_UNKNOWN_CMD"
    finally:
        ctx.signal_shutdown()
        t.join(timeout=2.0)


def test_nested_envelope_is_accepted(tmp_path):
    """Recorder claims to accept both flat and nested wire formats — verify."""
    ctx = FakeContext(machine_id="test", instance_name="rec")
    node = RecorderNode(ctx, {"name": "rec", "output_dir": str(tmp_path)})
    t = _run_node_in_thread(node)
    try:
        start = _query_command(
            ctx,
            "test",
            "rec",
            make_command_nested(
                "start_recording",
                {"topic_patterns": ["bubbaloop/global/test/cam/**"]},
            ),
        )
        assert start["status"] == "started"
    finally:
        ctx.signal_shutdown()
        t.join(timeout=2.0)
