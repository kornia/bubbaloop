#!/usr/bin/env bash
# validate.sh — Full system validation for bubbaloop
#
# Usage:
#   ./scripts/validate.sh          # Full validation (Rust + dashboard + clippy)
#   ./scripts/validate.sh --quick  # Rust only, skip dashboard
#   ./scripts/validate.sh --gemini # Full validation + Gemini CLI review

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

QUICK=false
GEMINI=false
FAILURES=0
TOTAL=0
RUST_TESTS=0
DASH_TESTS=0

for arg in "$@"; do
    case "$arg" in
        --quick) QUICK=true ;;
        --gemini) GEMINI=true ;;
    esac
done

step() {
    TOTAL=$((TOTAL + 1))
    printf "${YELLOW}[%d] %s${NC}\n" "$TOTAL" "$1"
}

pass() {
    printf "${GREEN}  PASS${NC} %s\n" "${1:-}"
}

fail() {
    FAILURES=$((FAILURES + 1))
    printf "${RED}  FAIL: %s${NC}\n" "$1"
}

cd "$ROOT_DIR"

# ══════════════════════════════════════════════════════════════════════
printf "${CYAN}── PHASE 1: Compilation ──${NC}\n"
# ══════════════════════════════════════════════════════════════════════

step "Cargo check (library)"
if cargo check --lib -p bubbaloop 2>&1; then
    pass
else
    fail "cargo check --lib failed"
fi

# ══════════════════════════════════════════════════════════════════════
printf "\n${CYAN}── PHASE 2: Rust Tests ──${NC}\n"
# ══════════════════════════════════════════════════════════════════════

step "Rust test suite"
OUTPUT=$(cargo test --lib -p bubbaloop 2>&1)
if echo "$OUTPUT" | grep -q "test result: ok"; then
    RUST_TESTS=$(echo "$OUTPUT" | grep -oP '\d+ passed' | grep -oP '\d+' || echo "0")
    pass "$RUST_TESTS tests"
else
    fail "Rust tests failed"
fi

# ══════════════════════════════════════════════════════════════════════
printf "\n${CYAN}── PHASE 3: Dashboard Tests ──${NC}\n"
# ══════════════════════════════════════════════════════════════════════

if ! $QUICK; then
    step "Dashboard test suite"
    OUTPUT=$(cd dashboard && npm test 2>&1)
    if echo "$OUTPUT" | grep -q "Tests.*passed"; then
        DASH_TESTS=$(echo "$OUTPUT" | grep -oP '\d+ passed' | grep -oP '\d+' || echo "0")
        DASH_FILES=$(echo "$OUTPUT" | grep -oP '\d+ passed' | head -1 | grep -oP '\d+' || echo "0")
        pass "$DASH_TESTS tests"
    else
        fail "Dashboard tests failed"
    fi
else
    printf "${YELLOW}  SKIP (--quick mode)${NC}\n"
fi

# ══════════════════════════════════════════════════════════════════════
printf "\n${CYAN}── PHASE 4: Clippy Lint ──${NC}\n"
# ══════════════════════════════════════════════════════════════════════

step "Clippy (zero warnings)"
if pixi run clippy 2>&1; then
    pass
else
    fail "clippy has warnings"
fi

# ══════════════════════════════════════════════════════════════════════
printf "\n${CYAN}── PHASE 5: Template Integrity ──${NC}\n"
# ══════════════════════════════════════════════════════════════════════

step "Template directories exist"
TPL_OK=true
for tpl_dir in templates/rust-node templates/python-node; do
    if [ ! -d "$tpl_dir" ]; then
        fail "Missing: $tpl_dir"
        TPL_OK=false
    fi
done
$TPL_OK && pass

step "Template variables use {{node_name}}"
VARS_OK=true
for tpl in templates/rust-node/node.yaml.template templates/python-node/node.yaml.template; do
    if [ -f "$tpl" ] && ! grep -q '{{node_name}}' "$tpl"; then
        fail "$tpl missing {{node_name}}"
        VARS_OK=false
    fi
done
$VARS_OK && pass

step "No complete=True in Python queryable"
if grep -v '^\s*#' templates/python-node/main.py.template 2>/dev/null | grep -q 'complete=True'; then
    fail "Python template has complete=True (blocks wildcard schema discovery)"
else
    pass
fi

step "No query.key_expr() method call (must be property, not method)"
if grep -v '^\s*#' templates/python-node/main.py.template 2>/dev/null | grep -q 'key_expr()'; then
    fail "Python template uses query.key_expr() — must be query.key_expr (property, no parens)"
else
    pass
fi

step "No .complete(true) in Rust queryable template"
if grep -v '^\s*//' templates/rust-node/src/node.rs.template 2>/dev/null | grep -q '\.complete(true)'; then
    fail "Rust template uses .complete(true) (blocks wildcard schema discovery)"
else
    pass
fi

step "Deployed Python nodes: no key_expr() bug"
DEPLOYED_OK=true
for py in "$HOME"/.bubbaloop/nodes/*/main.py "$HOME"/.bubbaloop/nodes/*/*/main.py; do
    if [ -f "$py" ]; then
        if grep -v '^\s*#' "$py" | grep -q 'key_expr()'; then
            fail "Deployed node has key_expr() bug: $py"
            DEPLOYED_OK=false
        fi
    fi
done 2>/dev/null
$DEPLOYED_OK && pass

step "Cargo.toml template: no git+path ambiguity"
if [ -f "templates/rust-node/Cargo.toml.template" ]; then
    if grep -n 'bubbaloop-schemas' templates/rust-node/Cargo.toml.template | grep -q 'git.*path\|path.*git'; then
        fail "Cargo.toml template has ambiguous git+path"
    else
        pass
    fi
fi

# ══════════════════════════════════════════════════════════════════════
printf "\n${CYAN}── PHASE 6: Schema Contract Validation ──${NC}\n"
# ══════════════════════════════════════════════════════════════════════

step "Rust template: DESCRIPTOR constant present"
if grep -q 'pub const DESCRIPTOR.*include_bytes' templates/rust-node/src/node.rs.template; then
    pass
else
    fail "Rust template missing DESCRIPTOR constant"
fi

step "Rust template: build.rs generates descriptor.bin"
if [ -f "templates/rust-node/build.rs.template" ]; then
    if grep -q 'file_descriptor_set_path' templates/rust-node/build.rs.template && \
       grep -q 'descriptor.bin' templates/rust-node/build.rs.template; then
        pass
    else
        fail "Rust build.rs.template doesn't generate descriptor.bin"
    fi
else
    fail "Rust template missing build.rs.template"
fi

step "Rust template: schema queryable present"
if grep -q '/schema' templates/rust-node/src/node.rs.template && \
   grep -q 'declare_queryable' templates/rust-node/src/node.rs.template; then
    pass
else
    fail "Rust template missing schema queryable"
fi

step "Python template: schema queryable present"
if grep -q '/schema' templates/python-node/main.py.template && \
   grep -q 'declare_queryable' templates/python-node/main.py.template; then
    pass
else
    fail "Python template missing schema queryable"
fi

step "Python template: descriptor.bin referenced"
if grep -q 'descriptor.bin' templates/python-node/main.py.template; then
    pass
else
    fail "Python template doesn't reference descriptor.bin"
fi

step "Deployed nodes: protos/ directories exist for protobuf nodes"
PROTOS_OK=true
for node_dir in "$HOME"/.bubbaloop/nodes/*/; do
    if [ -d "$node_dir" ]; then
        # Check if node uses protobuf (has .proto files or descriptor.bin)
        if find "$node_dir" -name "*.proto" -o -name "descriptor.bin" 2>/dev/null | grep -q .; then
            # If it uses protobuf, check for protos/ or descriptor.bin
            if [ ! -d "$node_dir/protos" ] && [ ! -f "$node_dir/descriptor.bin" ]; then
                fail "Protobuf node missing protos/: $(basename "$node_dir")"
                PROTOS_OK=false
            fi
        fi
    fi
done 2>/dev/null
$PROTOS_OK && pass

step "ARCHITECTURE.md: Schema Contract section present"
if grep -q '### Schema Contract (Protobuf Nodes)' ARCHITECTURE.md; then
    pass
else
    fail "ARCHITECTURE.md missing Schema Contract section"
fi

# ══════════════════════════════════════════════════════════════════════
printf "\n${CYAN}── PHASE 7: Contract Validation ──${NC}\n"
# ══════════════════════════════════════════════════════════════════════

step "Machine ID: single definition in daemon/util.rs"
MACHINE_ID_DEFS=$(grep -rn 'fn get_machine_id' crates/bubbaloop/src/daemon/ | wc -l)
if [ "$MACHINE_ID_DEFS" -eq 1 ]; then
    pass "get_machine_id() defined once"
else
    fail "get_machine_id() defined $MACHINE_ID_DEFS times (expected 1)"
fi

step "Templates: machine ID in topic prefix (BUBBALOOP_MACHINE_ID)"
SCOPE_OK=true
for tpl in templates/python-node/main.py.template templates/rust-node/src/node.rs.template; do
    if [ -f "$tpl" ]; then
        grep -q 'BUBBALOOP_MACHINE_ID' "$tpl" || { fail "$tpl missing BUBBALOOP_MACHINE_ID"; SCOPE_OK=false; }
    fi
done
$SCOPE_OK && pass

step "JSON API: NodeStateResponse has all 6 new fields"
API_FILE="crates/bubbaloop/src/daemon/zenoh_api.rs"
FIELDS_OK=true
for field in last_updated_ms health_status last_health_check_ms machine_id machine_hostname machine_ips; do
    if ! grep -q "pub $field" "$API_FILE"; then
        fail "NodeStateResponse missing field: $field"
        FIELDS_OK=false
    fi
done
$FIELDS_OK && pass

step "Proto: CONTRACT comment on NodeStatus enum"
if grep -q 'CONTRACT' crates/bubbaloop-schemas/protos/daemon.proto; then
    pass
else
    fail "daemon.proto missing CONTRACT comment on NodeStatus"
fi

step "Templates: manifest queryable present"
MANIFEST_OK=true
for tpl in templates/python-node/main.py.template templates/rust-node/src/node.rs.template; do
    if [ -f "$tpl" ] && ! grep -q 'manifest' "$tpl"; then
        fail "$tpl missing manifest queryable"
        MANIFEST_OK=false
    fi
done
$MANIFEST_OK && pass

step "Templates: health queryable present"
HEALTH_OK=true
for tpl in templates/python-node/main.py.template templates/rust-node/src/node.rs.template; do
    if [ -f "$tpl" ] && ! grep -q 'health' "$tpl"; then
        fail "$tpl missing health queryable"
        HEALTH_OK=false
    fi
done
$HEALTH_OK && pass

step "Templates: config queryable present"
CONFIG_OK=true
for tpl in templates/python-node/main.py.template templates/rust-node/src/node.rs.template; do
    if [ -f "$tpl" ] && ! grep -q '/config' "$tpl"; then
        fail "$tpl missing config queryable"
        CONFIG_OK=false
    fi
done
$CONFIG_OK && pass

step "Templates: command queryable present"
CMD_OK=true
for tpl in templates/python-node/main.py.template templates/rust-node/src/node.rs.template; do
    if [ -f "$tpl" ] && ! grep -q '/command' "$tpl"; then
        fail "$tpl missing command queryable"
        CMD_OK=false
    fi
done
$CMD_OK && pass

step "Templates: command_key in manifest"
CMD_KEY_OK=true
for tpl in templates/python-node/main.py.template templates/rust-node/src/node.rs.template; do
    if [ -f "$tpl" ] && ! grep -q 'command_key' "$tpl"; then
        fail "$tpl missing command_key in manifest"
        CMD_KEY_OK=false
    fi
done
$CMD_KEY_OK && pass

step "ARCHITECTURE.md: Command Contract section present"
if grep -q '### Command Contract' ARCHITECTURE.md; then
    pass
else
    fail "ARCHITECTURE.md missing Command Contract section"
fi

step "Systemd units: BUBBALOOP_MACHINE_ID injected"
if grep -q 'BUBBALOOP_MACHINE_ID' crates/bubbaloop/src/daemon/systemd.rs; then
    pass
else
    fail "systemd.rs doesn't inject BUBBALOOP_MACHINE_ID"
fi

# ══════════════════════════════════════════════════════════════════════
printf "\n${CYAN}── PHASE 8: Security Validation ──${NC}\n"
# ══════════════════════════════════════════════════════════════════════

step "Templates: scouting disabled"
SCOUT_OK=true
for tpl in templates/python-node/main.py.template templates/rust-node/src/node.rs.template; do
    if [ -f "$tpl" ] && ! grep -q 'scouting/multicast/enabled' "$tpl"; then
        fail "$tpl missing scouting disable"
        SCOUT_OK=false
    fi
done
$SCOUT_OK && pass

step "Templates: read BUBBALOOP_ZENOH_ENDPOINT"
ENDPOINT_OK=true
for tpl in templates/python-node/main.py.template templates/rust-node/src/node.rs.template; do
    if [ -f "$tpl" ] && ! grep -q 'BUBBALOOP_ZENOH_ENDPOINT' "$tpl"; then
        fail "$tpl missing BUBBALOOP_ZENOH_ENDPOINT"
        ENDPOINT_OK=false
    fi
done
$ENDPOINT_OK && pass

step "Python template: no 0.0.0.0 binding"
if grep -v '^\s*#' templates/python-node/main.py.template 2>/dev/null | grep -q '0\.0\.0\.0'; then
    fail "Python template binds to 0.0.0.0 (security risk)"
else
    pass
fi

step "Templates: security.acl_prefix in manifest"
ACL_OK=true
for tpl in templates/python-node/main.py.template templates/rust-node/src/node.rs.template; do
    if [ -f "$tpl" ] && ! grep -q 'acl_prefix' "$tpl"; then
        fail "$tpl missing acl_prefix in manifest"
        ACL_OK=false
    fi
done
$ACL_OK && pass

step "Systemd: Python sandbox directives present"
if grep -q 'ProtectHome' crates/bubbaloop/src/daemon/systemd.rs && \
   grep -q 'MemoryMax' crates/bubbaloop/src/daemon/systemd.rs && \
   grep -q 'RestrictSUIDSGID' crates/bubbaloop/src/daemon/systemd.rs; then
    pass
else
    fail "systemd.rs missing Python sandbox directives"
fi

step "Daemon: scouting disabled"
if grep -q 'scouting/multicast/enabled.*false' crates/bubbaloop/src/daemon/mod.rs; then
    pass
else
    fail "Daemon mod.rs doesn't disable scouting"
fi

# ══════════════════════════════════════════════════════════════════════
printf "\n${CYAN}── PHASE 9: Orphan Topic Lint ──${NC}\n"
# ══════════════════════════════════════════════════════════════════════

check_orphan_topics() {
    # Requires a running daemon; skips gracefully if none is reachable.
    python3 - <<'PYEOF'
import sys
import time
import collections

try:
    import zenoh
except ImportError:
    print("  SKIP  zenoh Python not available")
    sys.exit(0)

conf = zenoh.Config()
conf.insert_json5("mode", '"client"')
conf.insert_json5("connect/endpoints", '["tcp/127.0.0.1:7447"]')
conf.insert_json5("scouting/multicast/enabled", "false")
conf.insert_json5("scouting/gossip/enabled", "false")

try:
    session = zenoh.open(conf)
except Exception as e:
    print(f"  SKIP  no daemon reachable ({e})")
    sys.exit(0)

seen_keys = []

def on_sample(sample):
    seen_keys.append(str(sample.key_expr))

try:
    sub = session.declare_subscriber("bubbaloop/global/**", on_sample)
    time.sleep(6)
    sub.undeclare()
finally:
    session.close()

# Group keys by instance_name (4th path component: bubbaloop/global/{machine}/{instance}/...)
buckets = collections.defaultdict(list)
for key in seen_keys:
    parts = key.split("/")
    # bubbaloop / global / machine_id / instance_name / ...
    if len(parts) >= 4:
        bucket = "/".join(parts[:4])  # bubbaloop/global/{machine}/{instance}
        buckets[bucket].append(key)

orphans = []
for bucket, keys in buckets.items():
    has_health = any(k.endswith("/health") for k in keys)
    if not has_health:
        orphans.append((bucket, keys))

if not orphans:
    print(f"  PASS  {len(buckets)} instance bucket(s) seen, all have /health")
else:
    print(f"  FAIL  {len(orphans)} orphan bucket(s) — data topics with no /health (likely missing instance_name prefix):")
    for bucket, keys in orphans:
        print(f"    bucket: {bucket}")
        for k in keys[:5]:
            print(f"      {k}")
        if len(keys) > 5:
            print(f"      ... ({len(keys) - 5} more)")

# --- second pass: cross-check live instance names against nodes.json registry ---
import json as _json
import pathlib as _pathlib

nodes_json = _pathlib.Path.home() / ".bubbaloop" / "nodes.json"
if nodes_json.exists():
    try:
        registry_data = _json.loads(nodes_json.read_text())
        # nodes.json may be {"nodes": [...]} or a flat list/dict of entries
        if isinstance(registry_data, dict) and "nodes" in registry_data:
            entries = registry_data["nodes"]
        elif isinstance(registry_data, list):
            entries = registry_data
        elif isinstance(registry_data, dict):
            entries = list(registry_data.values())
        else:
            entries = []
        registered_names = set()
        for entry in entries:
            if isinstance(entry, dict):
                raw = entry.get("name_override") or entry.get("name") or entry.get("instance_name")
                if raw:
                    # Normalize hyphens to underscores to match topic sanitization
                    registered_names.add(raw.replace("-", "_"))
        # Extract live instance names (4th path component)
        live_instance_names = set()
        for bucket in buckets:
            parts = bucket.split("/")
            if len(parts) >= 4:
                live_instance_names.add(parts[3])
        # Filter out well-known non-node buckets (daemon, health, etc.)
        skip_names = {"daemon", "health", "agent"}
        unregistered = [
            name for name in live_instance_names
            if name not in registered_names and name not in skip_names
        ]
        if unregistered:
            print(f"  WARN  {len(unregistered)} unregistered instance(s) — live but not in nodes.json (possible typo in name: field):")
            for name in sorted(unregistered):
                print(f"    unregistered instance: {name}")
        else:
            print(f"  PASS  all {len(live_instance_names)} live instance(s) match nodes.json registry")
    except Exception as e:
        print(f"  SKIP  registry cross-check failed: {e}")
else:
    print("  SKIP  ~/.bubbaloop/nodes.json not found, skipping registry cross-check")

if orphans:
    sys.exit(1)
sys.exit(0)
PYEOF
}

step "Orphan topic lint (6s live Zenoh sniff)"
if check_orphan_topics; then
    : # pass already printed by the Python script
else
    FAILURES=$((FAILURES + 1))
fi

# ══════════════════════════════════════════════════════════════════════
printf "\n${CYAN}── PHASE 10: Gemini CLI Review ──${NC}\n"
# ══════════════════════════════════════════════════════════════════════

if $GEMINI; then
    step "Gemini review of zenoh_api.rs"
    if command -v gemini &>/dev/null; then
        GEMINI_OUT=$(NODE_OPTIONS="--max-old-space-size=4096" gemini -p \
            "Review this Rust file briefly. List max 3 actionable bugs or security issues only. Skip style suggestions." \
            < crates/bubbaloop/src/daemon/zenoh_api.rs 2>&1) || true
        echo "$GEMINI_OUT"
        pass "review complete"
    else
        fail "gemini CLI not found"
    fi
else
    printf "${YELLOW}  SKIP (use --gemini flag)${NC}\n"
fi

# ══════════════════════════════════════════════════════════════════════
# Summary
# ══════════════════════════════════════════════════════════════════════
echo ""
echo "================================================================="
printf "  Rust tests:      ${CYAN}%s${NC}\n" "$RUST_TESTS"
printf "  Dashboard tests: ${CYAN}%s${NC}\n" "${DASH_TESTS:-skipped}"
echo "================================================================="
if [ $FAILURES -eq 0 ]; then
    printf "${GREEN}  All %d checks passed.${NC}\n" "$TOTAL"
else
    printf "${RED}  %d of %d checks failed.${NC}\n" "$FAILURES" "$TOTAL"
fi
echo "================================================================="

exit $FAILURES
