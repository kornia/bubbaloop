#!/bin/bash
# Development stack management for bubbaloop
# Usage: ./scripts/dev.sh [start|stop|restart|status|logs]
#
# Manages: zenohd, zenoh-bridge (client mode), daemon, and optionally dashboard
# All processes run in background with logs to stderr.

set -euo pipefail

BUBBALOOP_DIR="${HOME}/.bubbaloop"
ZENOHD="${BUBBALOOP_DIR}/bin/zenohd"
BRIDGE="${BUBBALOOP_DIR}/bin/zenoh-bridge-remote-api"
ZENOH_CONFIG="${BUBBALOOP_DIR}/zenoh/zenohd.json5"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
DAEMON="${PROJECT_DIR}/target/release/bubbaloop"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

log() { echo -e "${GREEN}[dev]${NC} $*"; }
warn() { echo -e "${YELLOW}[dev]${NC} $*"; }
err() { echo -e "${RED}[dev]${NC} $*" >&2; }

pid_of() {
    pgrep -f "$1" 2>/dev/null | head -1
}

is_running() {
    [ -n "$(pid_of "$1")" ]
}

stop_all() {
    log "Stopping dev stack..."

    # Stop nodes first (graceful via daemon)
    if is_running "bubbaloop daemon"; then
        local nodes
        nodes=$("${DAEMON}" node list 2>/dev/null | awk 'NR>2 && $2=="Running" {print $1}')
        for node in $nodes; do
            "${DAEMON}" node stop "$node" 2>/dev/null && log "Stopped node: $node" || true
        done
        sleep 1
    fi

    # Kill dashboard
    local vite_pid
    vite_pid=$(pid_of "vite.*--host")
    if [ -n "$vite_pid" ]; then
        kill "$vite_pid" 2>/dev/null && log "Stopped dashboard (vite)" || true
    fi

    # Kill daemon
    local daemon_pid
    daemon_pid=$(pid_of "bubbaloop daemon run")
    if [ -n "$daemon_pid" ]; then
        kill "$daemon_pid" 2>/dev/null && log "Stopped daemon (PID $daemon_pid)" || true
    fi

    # Kill bridge
    local bridge_pid
    bridge_pid=$(pid_of "zenoh-bridge-remote-api")
    if [ -n "$bridge_pid" ]; then
        kill "$bridge_pid" 2>/dev/null && log "Stopped bridge (PID $bridge_pid)" || true
    fi

    # Kill zenohd
    local zenohd_pid
    zenohd_pid=$(pid_of "zenohd")
    if [ -n "$zenohd_pid" ]; then
        kill "$zenohd_pid" 2>/dev/null && log "Stopped zenohd (PID $zenohd_pid)" || true
    fi

    sleep 2
    log "All stopped."
}

start_all() {
    # Check binary exists
    if [ ! -x "$DAEMON" ]; then
        err "Daemon binary not found: $DAEMON"
        err "Run: cd $PROJECT_DIR && cargo build --release"
        exit 1
    fi

    if [ ! -x "$ZENOHD" ]; then
        err "zenohd not found: $ZENOHD"
        err "Run: bubbaloop doctor"
        exit 1
    fi

    if [ ! -x "$BRIDGE" ]; then
        err "zenoh-bridge not found: $BRIDGE"
        err "Run: bubbaloop doctor"
        exit 1
    fi

    log "Starting dev stack..."

    # 1. zenohd
    if is_running "zenohd"; then
        warn "zenohd already running (PID $(pid_of zenohd))"
    else
        "$ZENOHD" -c "$ZENOH_CONFIG" &>/dev/null &
        sleep 2
        if is_running "zenohd"; then
            log "zenohd started (PID $(pid_of zenohd))"
        else
            err "Failed to start zenohd"
            exit 1
        fi
    fi

    # 2. bridge (MUST be client mode for routing through zenohd)
    if is_running "zenoh-bridge-remote-api"; then
        warn "bridge already running (PID $(pid_of zenoh-bridge-remote-api))"
    else
        "$BRIDGE" --ws-port 10001 -e tcp/127.0.0.1:7447 -m client &>/dev/null &
        sleep 2
        if is_running "zenoh-bridge-remote-api"; then
            log "bridge started in CLIENT mode (PID $(pid_of zenoh-bridge-remote-api))"
        else
            err "Failed to start bridge"
            exit 1
        fi
    fi

    # 3. daemon
    if is_running "bubbaloop daemon"; then
        warn "daemon already running (PID $(pid_of 'bubbaloop daemon run'))"
    else
        cd "$PROJECT_DIR"
        "$DAEMON" daemon run &>/dev/null &
        sleep 3
        if is_running "bubbaloop daemon"; then
            log "daemon started (PID $(pid_of 'bubbaloop daemon run'))"
        else
            err "Failed to start daemon"
            exit 1
        fi
    fi

    log "Dev stack ready."
    show_status
}

start_dashboard() {
    if is_running "vite.*--host"; then
        warn "dashboard already running"
        return
    fi

    if [ ! -d "$PROJECT_DIR/dashboard/node_modules" ]; then
        log "Installing dashboard dependencies..."
        cd "$PROJECT_DIR/dashboard" && npm install --silent 2>/dev/null
    fi

    # Generate proto if missing
    if [ ! -f "$PROJECT_DIR/dashboard/src/proto/messages.pb.js" ]; then
        log "Generating proto files..."
        cd "$PROJECT_DIR/dashboard" && bash scripts/generate-proto.sh
    fi

    cd "$PROJECT_DIR/dashboard"
    npx vite --host 0.0.0.0 &>/dev/null &
    sleep 5
    if is_running "vite"; then
        local ip
        ip=$(tailscale ip -4 2>/dev/null || hostname -I | awk '{print $1}')
        log "Dashboard running at: http://${ip}:5173"
    else
        err "Failed to start dashboard"
    fi
}

start_nodes() {
    if ! is_running "bubbaloop daemon"; then
        err "Daemon not running. Start the stack first."
        return 1
    fi

    local nodes
    nodes=$("${DAEMON}" node list 2>/dev/null | awk 'NR>2 && $2!="Running" && $1!="" {print $1}')
    for node in $nodes; do
        "${DAEMON}" node start "$node" 2>/dev/null && log "Started node: $node" || warn "Failed to start: $node"
    done
}

show_status() {
    echo ""
    printf "  %-25s %s\n" "COMPONENT" "STATUS"
    printf "  %-25s %s\n" "─────────────────────────" "──────────────────────"

    if is_running "zenohd"; then
        printf "  %-25s ${GREEN}running${NC} (PID %s)\n" "zenohd" "$(pid_of zenohd)"
    else
        printf "  %-25s ${RED}stopped${NC}\n" "zenohd"
    fi

    if is_running "zenoh-bridge-remote-api"; then
        printf "  %-25s ${GREEN}running${NC} (PID %s)\n" "bridge (client mode)" "$(pid_of zenoh-bridge-remote-api)"
    else
        printf "  %-25s ${RED}stopped${NC}\n" "bridge"
    fi

    if is_running "bubbaloop daemon"; then
        printf "  %-25s ${GREEN}running${NC} (PID %s)\n" "daemon" "$(pid_of 'bubbaloop daemon run')"
    else
        printf "  %-25s ${RED}stopped${NC}\n" "daemon"
    fi

    if is_running "vite"; then
        local ip
        ip=$(tailscale ip -4 2>/dev/null || hostname -I | awk '{print $1}')
        printf "  %-25s ${GREEN}running${NC} → http://%s:5173\n" "dashboard" "$ip"
    else
        printf "  %-25s ${YELLOW}not started${NC} (run: dev.sh dashboard)\n" "dashboard"
    fi

    echo ""

    # Show nodes if daemon is running
    if is_running "bubbaloop daemon"; then
        "${DAEMON}" node list 2>/dev/null || true
    fi

    # Memory check for known processes
    echo ""
    local camera_pid
    camera_pid=$(pid_of "rtsp_camera_node")
    if [ -n "$camera_pid" ]; then
        local rss
        rss=$(ps -o rss= -p "$camera_pid" 2>/dev/null)
        printf "  %-25s RSS: %s KB\n" "rtsp-camera memory" "$rss"
    fi
}

show_logs() {
    local component="${1:-daemon}"
    case "$component" in
        camera|rtsp-camera)
            "${DAEMON}" node logs rtsp-camera 2>&1 | tail -20
            ;;
        *)
            err "Logs for '$component' — daemon runs with stderr redirected."
            err "For node logs: dev.sh logs camera"
            ;;
    esac
}

case "${1:-status}" in
    start)
        start_all
        start_nodes
        ;;
    stop)
        stop_all
        ;;
    restart)
        stop_all
        start_all
        start_nodes
        ;;
    dashboard)
        start_dashboard
        ;;
    nodes)
        start_nodes
        ;;
    status)
        show_status
        ;;
    logs)
        show_logs "${2:-daemon}"
        ;;
    *)
        echo "Usage: $0 {start|stop|restart|status|dashboard|nodes|logs [component]}"
        exit 1
        ;;
esac
