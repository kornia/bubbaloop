#!/bin/bash
# Run coordination scenario tests
#
# Prerequisites:
#   1. Start zenohd in another terminal: zenohd
#   2. Ensure zenohd is running on tcp/127.0.0.1:7447

set -e

echo "=================================="
echo "Coordination Scenario Tests"
echo "=================================="
echo ""

# Check if zenohd is running
if ! pgrep -x "zenohd" > /dev/null; then
    echo "ERROR: zenohd is not running"
    echo "Please start zenohd in another terminal:"
    echo "  zenohd"
    exit 1
fi

echo "✓ zenohd is running"
echo ""

# Check if zenohd is listening on the expected port
if ! netstat -tuln 2>/dev/null | grep -q ":7447" && ! ss -tuln 2>/dev/null | grep -q ":7447"; then
    echo "WARNING: Could not verify zenohd is listening on port 7447"
    echo "Tests may fail if zenohd is not properly configured"
    echo ""
fi

echo "Running coordination scenario tests..."
echo ""

# Run tests with nice output
cargo test \
    --package bubbaloop-daemon \
    --test coordination_scenarios \
    -- \
    --ignored \
    --nocapture \
    --test-threads=1

echo ""
echo "=================================="
echo "All tests completed!"
echo "=================================="
