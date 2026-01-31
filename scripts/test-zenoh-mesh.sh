#!/bin/bash
# Test script to verify Zenoh mesh connectivity for inter-Jetson communication
#
# This script verifies that:
# 1. Local router is running
# 2. Connection to central router works
# 3. Pub/sub works across the mesh
# 4. Queryables work across the mesh
#
# Usage:
#   On Jetson 1: ./scripts/test-zenoh-mesh.sh publisher
#   On Jetson 2: ./scripts/test-zenoh-mesh.sh subscriber
#   Or for self-test: ./scripts/test-zenoh-mesh.sh selftest

set -e

ZENOH_ENDPOINT="${BUBBALOOP_ZENOH_ENDPOINT:-tcp/127.0.0.1:7447}"
TEST_TOPIC="bubbaloop/test/mesh-verify"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_ok() { echo -e "${GREEN}[OK]${NC} $1"; }
log_fail() { echo -e "${RED}[FAIL]${NC} $1"; }
log_info() { echo -e "${YELLOW}[INFO]${NC} $1"; }

check_zenoh_tools() {
    log_info "Checking Zenoh CLI tools..."

    if ! command -v z_pub &> /dev/null; then
        log_fail "z_pub not found. Install zenoh CLI tools:"
        echo "  cargo install zenoh-cli"
        exit 1
    fi

    if ! command -v z_sub &> /dev/null; then
        log_fail "z_sub not found"
        exit 1
    fi

    if ! command -v z_get &> /dev/null; then
        log_fail "z_get not found"
        exit 1
    fi

    log_ok "Zenoh CLI tools available"
}

check_router_connection() {
    log_info "Testing connection to router at $ZENOH_ENDPOINT..."

    # Try to connect and list admin space
    timeout 5 z_get -e "$ZENOH_ENDPOINT" '@/router/*' 2>/dev/null && {
        log_ok "Connected to router"
        return 0
    } || {
        log_fail "Cannot connect to router at $ZENOH_ENDPOINT"
        echo "  Make sure zenohd is running:"
        echo "    zenohd -c configs/zenoh/jetson-router.json5"
        return 1
    }
}

test_local_pubsub() {
    log_info "Testing local pub/sub..."

    # Start subscriber in background
    timeout 5 z_sub -e "$ZENOH_ENDPOINT" "$TEST_TOPIC" > /tmp/zenoh-test-sub.txt 2>&1 &
    SUB_PID=$!
    sleep 1

    # Publish test message
    echo "test-message-$(date +%s)" | timeout 3 z_pub -e "$ZENOH_ENDPOINT" "$TEST_TOPIC"
    sleep 1

    # Check if message was received
    kill $SUB_PID 2>/dev/null || true

    if grep -q "test-message" /tmp/zenoh-test-sub.txt 2>/dev/null; then
        log_ok "Local pub/sub works"
        return 0
    else
        log_fail "Local pub/sub failed"
        cat /tmp/zenoh-test-sub.txt 2>/dev/null
        return 1
    fi
}

run_publisher() {
    log_info "Running as publisher on $TEST_TOPIC..."
    log_info "Press Ctrl+C to stop"

    i=0
    while true; do
        msg="message-$i-from-$(hostname)-$(date +%s)"
        echo "$msg" | z_pub -e "$ZENOH_ENDPOINT" "$TEST_TOPIC"
        echo "Published: $msg"
        i=$((i + 1))
        sleep 2
    done
}

run_subscriber() {
    log_info "Subscribing to $TEST_TOPIC..."
    log_info "Waiting for messages from other Jetsons. Press Ctrl+C to stop."

    z_sub -e "$ZENOH_ENDPOINT" "$TEST_TOPIC"
}

test_queryable() {
    log_info "Testing queryable (request/reply)..."

    QUERY_KEY="bubbaloop/test/query-verify"

    # Start queryable in background (simulates Jetson 2's service)
    (
        z_queryable -e "$ZENOH_ENDPOINT" "$QUERY_KEY" <<< "response-from-$(hostname)"
    ) &
    QUERY_PID=$!
    sleep 1

    # Send query (simulates Jetson 1 querying Jetson 2)
    RESPONSE=$(timeout 3 z_get -e "$ZENOH_ENDPOINT" "$QUERY_KEY" 2>/dev/null)

    kill $QUERY_PID 2>/dev/null || true

    if echo "$RESPONSE" | grep -q "response-from"; then
        log_ok "Queryable works: $RESPONSE"
        return 0
    else
        log_fail "Queryable test failed"
        return 1
    fi
}

show_router_peers() {
    log_info "Checking connected router peers..."

    # Query the admin space for peer information
    z_get -e "$ZENOH_ENDPOINT" '@/router/*/linkstate/**' 2>/dev/null | head -20 || {
        log_info "Could not query router admin space (this is normal for some configs)"
    }
}

selftest() {
    echo "========================================"
    echo "Zenoh Mesh Connectivity Self-Test"
    echo "========================================"
    echo "Endpoint: $ZENOH_ENDPOINT"
    echo ""

    check_zenoh_tools
    check_router_connection || exit 1
    test_local_pubsub || exit 1
    test_queryable || exit 1
    show_router_peers

    echo ""
    echo "========================================"
    log_ok "All local tests passed!"
    echo "========================================"
    echo ""
    echo "To test cross-Jetson communication:"
    echo "  On Jetson 1: $0 publisher"
    echo "  On Jetson 2: $0 subscriber"
    echo ""
    echo "Both Jetsons should see messages from each other."
}

case "${1:-selftest}" in
    publisher|pub)
        check_zenoh_tools
        check_router_connection || exit 1
        run_publisher
        ;;
    subscriber|sub)
        check_zenoh_tools
        check_router_connection || exit 1
        run_subscriber
        ;;
    selftest|test)
        selftest
        ;;
    peers)
        check_zenoh_tools
        show_router_peers
        ;;
    *)
        echo "Usage: $0 {selftest|publisher|subscriber|peers}"
        echo ""
        echo "Commands:"
        echo "  selftest   - Run local connectivity tests (default)"
        echo "  publisher  - Publish test messages (run on Jetson 1)"
        echo "  subscriber - Subscribe to test messages (run on Jetson 2)"
        echo "  peers      - Show connected router peers"
        exit 1
        ;;
esac
