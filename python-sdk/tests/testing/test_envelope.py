"""Tests for envelope builders."""

from __future__ import annotations

import json

import cbor2

from bubbaloop_sdk.testing import make_command, make_command_nested, make_envelope


def test_make_envelope_round_trips():
    raw = make_envelope(
        "bubbaloop://schemas/sensor/CompressedImage/v1",
        body={"data": b"fake-h264", "format": "h264"},
        source_instance="cam1",
        ts_ns=1_700_000_000_000_000_000,
        monotonic_seq=42,
    )
    env = cbor2.loads(raw)
    assert env["header"]["schema_uri"] == "bubbaloop://schemas/sensor/CompressedImage/v1"
    assert env["header"]["source_instance"] == "cam1"
    assert env["header"]["ts_ns"] == 1_700_000_000_000_000_000
    assert env["header"]["monotonic_seq"] == 42
    assert env["body"]["data"] == b"fake-h264"


def test_make_envelope_defaults_ts_ns():
    raw = make_envelope("uri", body={})
    env = cbor2.loads(raw)
    assert env["header"]["ts_ns"] > 0
    assert env["header"]["monotonic_seq"] == 0
    assert env["header"]["source_instance"] == "test_instance"


def test_make_command_flat():
    raw = make_command("start_recording", topic_patterns=["a/**"])
    obj = json.loads(raw)
    assert obj == {"command": "start_recording", "topic_patterns": ["a/**"]}


def test_make_command_nested():
    raw = make_command_nested("start_recording", {"topic_patterns": ["a/**"]})
    obj = json.loads(raw)
    assert obj == {"command": "start_recording", "params": {"topic_patterns": ["a/**"]}}


def test_make_command_nested_defaults_to_empty_params():
    raw = make_command_nested("get_status")
    obj = json.loads(raw)
    assert obj == {"command": "get_status", "params": {}}
