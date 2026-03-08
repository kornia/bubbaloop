#!/usr/bin/env python3
"""End-to-end tests for bubbaloop Physical AI Memory & Mission features.

Usage:
    python3 scripts/e2e-test.py [--binary PATH]

Tests the MCP stdio interface for all new v0.0.11 tools:
  - Beliefs (update_belief, get_belief)
  - World state (list_world_state)
  - Missions (list_missions, pause_mission, resume_mission, cancel_mission)
  - Constraints (register_constraint, list_constraints)
  - Context (configure_context)
"""

import subprocess
import json
import sys
import os
import time
import argparse
import tempfile
import shutil

BINARY = "./target/debug/bubbaloop"
PASS = "\033[32mPASS\033[0m"
FAIL = "\033[31mFAIL\033[0m"
SKIP = "\033[33mSKIP\033[0m"


class McpSession:
    def __init__(self, binary):
        self.proc = subprocess.Popen(
            [binary, "mcp", "--stdio"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self._req_id = 1
        self._initialize()

    def _initialize(self):
        self._send_raw(
            {
                "jsonrpc": "2.0",
                "id": self._next_id(),
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "e2e-test", "version": "0.1"},
                },
            }
        )
        self.proc.stdin.write(
            b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n'
        )
        self.proc.stdin.flush()

    def _next_id(self):
        self._req_id += 1
        return self._req_id

    def _send_raw(self, msg):
        data = json.dumps(msg) + "\n"
        self.proc.stdin.write(data.encode())
        self.proc.stdin.flush()
        line = self.proc.stdout.readline().decode().strip()
        return json.loads(line) if line else None

    def call(self, tool, args=None):
        return self._send_raw(
            {
                "jsonrpc": "2.0",
                "id": self._next_id(),
                "method": "tools/call",
                "params": {"name": tool, "arguments": args or {}},
            }
        )

    def tools(self):
        r = self._send_raw(
            {"jsonrpc": "2.0", "id": self._next_id(), "method": "tools/list", "params": {}}
        )
        return [t["name"] for t in r.get("result", {}).get("tools", [])]

    def close(self):
        self.proc.terminate()
        self.proc.wait(timeout=5)


def result_text(r):
    if r is None:
        return None, "no response"
    if "error" in r:
        return None, f"jsonrpc error: {r['error']}"
    content = r.get("result", {}).get("content", [])
    if isinstance(content, list) and content:
        return content[0].get("text", ""), None
    return str(content), None


def assert_ok(r, label):
    text, err = result_text(r)
    if err:
        print(f"  {FAIL}  {label}: {err}")
        return False
    print(f"  {PASS}  {label}")
    return True


def assert_contains(r, substring, label):
    text, err = result_text(r)
    if err:
        print(f"  {FAIL}  {label}: {err}")
        return False
    if substring not in text:
        print(f"  {FAIL}  {label}: expected '{substring}' in response, got: {text[:200]}")
        return False
    print(f"  {PASS}  {label}")
    return True


def assert_error(r, label):
    if r and "error" in r.get("result", {}).get("content", [{}])[0].get("text", ""):
        print(f"  {PASS}  {label} (expected error)")
        return True
    if r and "error" in r:
        print(f"  {PASS}  {label} (jsonrpc error as expected)")
        return True
    text, _ = result_text(r)
    print(f"  {FAIL}  {label}: expected error, got: {text[:100] if text else r}")
    return False


def run_tests(binary):
    failures = 0
    total = 0

    print(f"\nBinary: {binary}")
    print("=" * 60)

    # ── Connectivity ──────────────────────────────────────────
    print("\n[1] Connectivity")
    mcp = McpSession(binary)
    available = mcp.tools()
    required = [
        "update_belief", "get_belief", "list_world_state",
        "list_missions", "pause_mission", "resume_mission", "cancel_mission",
        "register_constraint", "list_constraints", "configure_context",
    ]
    for t in required:
        total += 1
        if t in available:
            print(f"  {PASS}  tool '{t}' advertised")
        else:
            print(f"  {FAIL}  tool '{t}' NOT in tools/list")
            failures += 1
    mcp.close()

    # ── Beliefs ───────────────────────────────────────────────
    print("\n[2] Beliefs")
    mcp = McpSession(binary)

    total += 1
    r = mcp.call("update_belief", {
        "subject": "front_door_camera",
        "predicate": "is_reliable",
        "value": "true",
        "confidence": 0.95,
        "source": "heartbeat_monitor",
    })
    if not assert_contains(r, "updated", "update_belief creates belief"): failures += 1

    total += 1
    r = mcp.call("get_belief", {"subject": "front_door_camera", "predicate": "is_reliable"})
    if not assert_contains(r, "front_door_camera", "get_belief retrieves belief"): failures += 1

    total += 1
    r = mcp.call("get_belief", {"subject": "front_door_camera", "predicate": "is_reliable"})
    text, _ = result_text(r)
    try:
        data = json.loads(text) if text else {}
        if abs(data.get("confidence", 0) - 0.95) < 0.001:
            print(f"  {PASS}  belief confidence stored correctly (0.95)")
        else:
            print(f"  {FAIL}  belief confidence wrong: {data.get('confidence')}")
            failures += 1
    except Exception as e:
        print(f"  {FAIL}  belief parse error: {e}")
        failures += 1

    # Update same belief — confirmation_count should increment
    total += 1
    mcp.call("update_belief", {
        "subject": "front_door_camera",
        "predicate": "is_reliable",
        "value": "true",
        "confidence": 0.98,
    })
    r = mcp.call("get_belief", {"subject": "front_door_camera", "predicate": "is_reliable"})
    text, _ = result_text(r)
    try:
        data = json.loads(text) if text else {}
        if data.get("confirmation_count", 0) >= 2:
            print(f"  {PASS}  belief confirmation_count increments on re-update")
        else:
            print(f"  {FAIL}  confirmation_count={data.get('confirmation_count')}, expected >=2")
            failures += 1
    except Exception as e:
        print(f"  {FAIL}  {e}")
        failures += 1

    total += 1
    r = mcp.call("get_belief", {"subject": "nonexistent", "predicate": "nothing"})
    text, err = result_text(r)
    if (text and "not found" in text.lower()) or err:
        print(f"  {PASS}  get_belief unknown key returns not-found")
    else:
        print(f"  {FAIL}  expected not-found for unknown belief, got: {text}")
        failures += 1

    mcp.close()

    # ── World State ───────────────────────────────────────────
    print("\n[3] World State")
    mcp = McpSession(binary)

    total += 1
    r = mcp.call("list_world_state", {})
    text, err = result_text(r)
    if err:
        print(f"  {FAIL}  list_world_state error: {err}")
        failures += 1
    else:
        print(f"  {PASS}  list_world_state returns (content: {text[:80]})")

    mcp.close()

    # ── Missions ──────────────────────────────────────────────
    print("\n[4] Missions (file-driven — testing list/control tools)")
    mcp = McpSession(binary)

    total += 1
    r = mcp.call("list_missions", {})
    if not assert_ok(r, "list_missions returns without error"): failures += 1

    # These should fail gracefully with unknown mission ID
    for tool in ["pause_mission", "resume_mission", "cancel_mission"]:
        total += 1
        r = mcp.call(tool, {"mission_id": "nonexistent-mission-id"})
        text, err = result_text(r)
        if (text and ("not found" in text.lower() or "error" in text.lower())) or err:
            print(f"  {PASS}  {tool} unknown ID → graceful error")
        else:
            print(f"  {FAIL}  {tool} unknown ID did not error: {text}")
            failures += 1

    mcp.close()

    # ── Constraints ───────────────────────────────────────────
    print("\n[5] Constraints (mission-scoped)")
    mcp = McpSession(binary)

    total += 1
    r = mcp.call("list_constraints", {"mission_id": "test-mission-123"})
    text, err = result_text(r)
    if err:
        print(f"  {FAIL}  list_constraints error: {err}")
        failures += 1
    else:
        print(f"  {PASS}  list_constraints (empty mission) → {text[:80]}")

    total += 1
    # Workspace uses tuple format: {"x": [min, max], "y": [min, max], "z": [min, max]}
    r = mcp.call("register_constraint", {
        "mission_id": "test-mission-123",
        "constraint_type": "workspace",
        "params_json": json.dumps({"x": [-1.0, 1.0], "y": [-1.0, 1.0], "z": [0.0, 2.0]}),
    })
    text, err = result_text(r)
    if err or (text and text.startswith("Error:")):
        print(f"  {FAIL}  register_constraint workspace bounds: {err or text}")
        failures += 1
    else:
        print(f"  {PASS}  register_constraint workspace bounds")

    total += 1
    r = mcp.call("list_constraints", {"mission_id": "test-mission-123"})
    # Rust enum serializes as "Workspace" (PascalCase variant name)
    if not assert_contains(r, "Workspace", "list_constraints shows registered constraint"): failures += 1

    mcp.close()

    # ── Context Provider ──────────────────────────────────────
    print("\n[6] Context Provider (Zenoh→world state wiring)")
    mcp = McpSession(binary)

    total += 1
    # configure_context wires a Zenoh topic pattern to world state
    r = mcp.call("configure_context", {
        "mission_id": "test-mission-123",
        "topic_pattern": "bubbaloop/**/vision/detections",
        "world_state_key_template": "{label}.location",
        "value_field": "label",
        "filter": "confidence>0.8",
    })
    text, err = result_text(r)
    if err or (text and text.startswith("Error:")):
        print(f"  {FAIL}  configure_context: {err or text}")
        failures += 1
    else:
        print(f"  {PASS}  configure_context wires topic to world state")

    mcp.close()

    # ── Summary ───────────────────────────────────────────────
    print("\n" + "=" * 60)
    passed = total - failures
    status = PASS if failures == 0 else FAIL
    print(f"Result: {status}  {passed}/{total} tests passed")
    return failures


def main():
    parser = argparse.ArgumentParser(description="Bubbaloop E2E tests")
    parser.add_argument("--binary", default=BINARY, help="Path to bubbaloop binary")
    args = parser.parse_args()

    binary = args.binary
    if not os.path.exists(binary):
        print(f"Binary not found: {binary}")
        print("Build with: cargo build --bin bubbaloop")
        sys.exit(1)

    failures = run_tests(binary)
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
