#!/usr/bin/env python3
"""camera-snapshot node — polls an IP camera's JPEG endpoint and publishes frames."""

import argparse
import base64
import json
import logging
import os
import signal
import sys
import time
import urllib.error
import urllib.request

import yaml

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
log = logging.getLogger("camera-snapshot")

# Minimal valid 1x1 pixel white JPEG — no external deps needed for mock mode.
_MOCK_JPEG = base64.b64decode(
    "/9j/4AAQSkZJRgABAQEASABIAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8U"
    "HRofHh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/2wBDAQkJCQwLDBgN"
    "DRgyIRwhMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIy"
    "MjIyMjL/wAARCAABAAEDASIAAhEBAxEB/8QAFgABAQEAAAAAAAAAAAAAAAAABgUE/8QAFBAB"
    "AAAAAAAAAAAAAAAAAAAAAP/EABQBAQAAAAAAAAAAAAAAAAAAAAD/xAAUEQEAAAAAAAAAAAAA"
    "AAAAAAAA/9oADAMBAAIRAxEAPwCwABmX/9k="
)


def load_config(path: str) -> dict:
    with open(path) as f:
        return yaml.safe_load(f)


def fetch_snapshot(base_url: str, timeout: int = 10) -> bytes | None:
    """Fetch a JPEG snapshot from the camera URL. Returns bytes or None on error."""
    # Try {base_url}/snapshot first, then {base_url} directly.
    urls = [base_url.rstrip("/") + "/snapshot", base_url]
    for url in urls:
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "bubbaloop/1.0"})
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                data = resp.read()
                if data:
                    return data
        except urllib.error.URLError as e:
            log.debug("fetch %s failed: %s", url, e)
        except Exception as e:
            log.debug("fetch %s error: %s", url, e)
    return None


def main():
    parser = argparse.ArgumentParser(description="HTTP snapshot camera node")
    parser.add_argument("-c", "--config", default="config.yaml", help="Config file path")
    parser.add_argument(
        "-e",
        "--endpoint",
        default=os.environ.get("ZENOH_ENDPOINT", "tcp/127.0.0.1:7447"),
        help="Zenoh endpoint (default: tcp/127.0.0.1:7447)",
    )
    args = parser.parse_args()

    config = load_config(args.config)
    name = config.get("name", "camera-snapshot")
    url = config.get("url", "http://127.0.0.1")
    interval_secs = float(config.get("interval_secs", 1.0))
    scope = os.environ.get("BUBBALOOP_SCOPE", "local")
    machine_id = os.environ.get("BUBBALOOP_MACHINE_ID", "localhost").replace("-", "_")
    mock_mode = str(url).startswith("mock://")

    import zenoh

    conf = zenoh.Config()
    conf.insert_json5("mode", '"client"')
    conf.insert_json5("connect/endpoints", json.dumps([args.endpoint]))
    session = zenoh.open(conf)

    snapshot_topic = f"bubbaloop/{scope}/{machine_id}/{name}/snapshot"
    health_topic = f"bubbaloop/{scope}/{machine_id}/health/{name}"

    snapshot_pub = session.declare_publisher(snapshot_topic)
    health_pub = session.declare_publisher(health_topic)

    log.info("camera-snapshot started (mode=%s, url=%s, interval=%.1fs)", "mock" if mock_mode else "live", url, interval_secs)
    log.info("Publishing snapshots to: %s", snapshot_topic)

    running = True
    last_health = 0.0
    frame_count = 0

    def handle_signal(signum, frame):
        nonlocal running
        log.info("Shutdown signal received")
        running = False

    signal.signal(signal.SIGINT, handle_signal)
    signal.signal(signal.SIGTERM, handle_signal)

    while running:
        tick_start = time.monotonic()

        # Publish health every 5 seconds.
        if tick_start - last_health >= 5.0:
            health_pub.put(b"ok")
            last_health = tick_start

        if mock_mode:
            # Generate a deterministic 1x1 JPEG with frame counter embedded in logs.
            frame_bytes = _MOCK_JPEG
            snapshot_pub.put(frame_bytes)
            log.info("Published mock frame #%d (%d bytes)", frame_count, len(frame_bytes))
        else:
            frame_bytes = fetch_snapshot(url)
            if frame_bytes is not None:
                snapshot_pub.put(frame_bytes)
                log.info("Published frame #%d (%d bytes)", frame_count, len(frame_bytes))
            else:
                log.warning("Camera at %s unreachable — skipping frame #%d", url, frame_count)

        frame_count += 1

        elapsed = time.monotonic() - tick_start
        sleep_time = max(0.0, interval_secs - elapsed)
        if sleep_time > 0:
            time.sleep(sleep_time)

    log.info("camera-snapshot stopped after %d frames", frame_count)
    snapshot_pub.undeclare()
    health_pub.undeclare()
    session.close()


if __name__ == "__main__":
    main()
