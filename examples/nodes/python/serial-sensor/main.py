#!/usr/bin/env python3
"""serial-sensor node — reads a serial port line-by-line and publishes parsed JSON to Zenoh."""

# Real serial usage (requires: pip install pyserial):
#
#   import serial
#   ser = serial.Serial(port, baud_rate, timeout=1)
#   while running:
#       line = ser.readline().decode('utf-8', errors='replace').strip()
#       if line:
#           process_line(line)
#
# This node runs in mock mode when port == "mock" (no pyserial required).

import argparse
import json
import logging
import math
import os
import signal
import time

import yaml

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
log = logging.getLogger("serial-sensor")


def load_config(path: str) -> dict:
    with open(path) as f:
        return yaml.safe_load(f)


def parse_line(line: str) -> dict | None:
    """Parse a line as JSON or CSV. Returns dict or None on parse failure."""
    line = line.strip()
    if not line:
        return None

    # Try JSON first.
    try:
        value = json.loads(line)
        if isinstance(value, dict):
            return value
        # Wrap scalar JSON values.
        return {"value": value}
    except json.JSONDecodeError:
        pass

    # Fallback: parse as CSV (val1,val2,...).
    parts = line.split(",")
    try:
        values = [float(p.strip()) for p in parts]
        return {"values": values}
    except ValueError:
        pass

    # Last resort: treat as raw string.
    return {"raw": line}


def mock_line(seq: int) -> str:
    """Generate a realistic simulated sensor reading as a JSON line."""
    # Simulate temperature + humidity + pressure sensor (like BME280).
    t = 22.5 + 2.0 * math.sin(seq * 0.1)
    h = 55.0 + 5.0 * math.cos(seq * 0.07)
    p = 1013.25 + 0.5 * math.sin(seq * 0.03)
    return json.dumps({
        "temperature_c": round(t, 2),
        "humidity_pct": round(h, 2),
        "pressure_hpa": round(p, 2),
    })


def open_serial(port: str, baud_rate: int):
    """Open a real serial port. Returns the serial.Serial object or raises."""
    try:
        import serial  # type: ignore[import]
    except ImportError:
        raise ImportError(
            "pyserial not installed. Run: pip install pyserial\n"
            "Or use port=mock in config for simulation mode."
        )
    return serial.Serial(port, baud_rate, timeout=1)


def main():
    parser = argparse.ArgumentParser(description="Serial port sensor node")
    parser.add_argument("-c", "--config", default="config.yaml", help="Config file path")
    parser.add_argument(
        "-e",
        "--endpoint",
        default=os.environ.get("ZENOH_ENDPOINT", "tcp/127.0.0.1:7447"),
        help="Zenoh endpoint (default: tcp/127.0.0.1:7447)",
    )
    args = parser.parse_args()

    config = load_config(args.config)
    name = config.get("name", "serial-sensor")
    port = config.get("port", "mock")
    baud_rate = int(config.get("baud_rate", 115200))
    interval_secs = float(config.get("interval_secs", 0.5))
    scope = os.environ.get("BUBBALOOP_SCOPE", "local")
    machine_id = os.environ.get("BUBBALOOP_MACHINE_ID", "localhost").replace("-", "_")
    mock_mode = port == "mock"

    import zenoh

    conf = zenoh.Config()
    conf.insert_json5("mode", '"client"')
    conf.insert_json5("connect/endpoints", json.dumps([args.endpoint]))
    session = zenoh.open(conf)

    data_topic = f"bubbaloop/{scope}/{machine_id}/{name}/data"
    health_topic = f"bubbaloop/{scope}/{machine_id}/health/{name}"

    data_pub = session.declare_publisher(data_topic)
    health_pub = session.declare_publisher(health_topic)

    log.info("serial-sensor started (port=%s, baud=%d, interval=%.2fs)", port, baud_rate, interval_secs)
    log.info("Publishing to: %s", data_topic)

    running = True
    last_health = 0.0
    seq = 0
    ser = None

    def handle_signal(signum, frame):
        nonlocal running
        log.info("Shutdown signal received")
        running = False

    signal.signal(signal.SIGINT, handle_signal)
    signal.signal(signal.SIGTERM, handle_signal)

    if not mock_mode:
        try:
            ser = open_serial(port, baud_rate)
            log.info("Opened serial port %s at %d baud", port, baud_rate)
        except Exception as e:
            log.error("Failed to open serial port %s: %s", port, e)
            session.close()
            sys.exit(1)

    while running:
        tick_start = time.monotonic()

        # Publish health every 5 seconds.
        if tick_start - last_health >= 5.0:
            health_pub.put(b"ok")
            last_health = tick_start

        if mock_mode:
            line = mock_line(seq)
            parsed = parse_line(line)
        else:
            try:
                raw = ser.readline()
                line = raw.decode("utf-8", errors="replace")
                parsed = parse_line(line)
            except Exception as e:
                log.warning("Serial read error: %s", e)
                parsed = None

        if parsed is not None:
            parsed["_seq"] = seq
            parsed["_ts_ms"] = int(time.time() * 1000)
            payload = json.dumps(parsed).encode()
            data_pub.put(payload)
            log.info("Published seq=%d: %s", seq, parsed)
        else:
            log.debug("Empty or unparseable line at seq=%d", seq)

        seq += 1

        if mock_mode:
            elapsed = time.monotonic() - tick_start
            sleep_time = max(0.0, interval_secs - elapsed)
            if sleep_time > 0:
                time.sleep(sleep_time)

    log.info("serial-sensor stopped after %d readings", seq)
    data_pub.undeclare()
    health_pub.undeclare()
    if ser is not None:
        ser.close()
    session.close()


if __name__ == "__main__":
    import sys
    main()
