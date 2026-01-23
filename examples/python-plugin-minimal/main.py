#!/usr/bin/env python3
"""
Minimal Bubbaloop Plugin Example

Copy this file and modify:
1. Config class - add your config fields
2. data dict in run() - define your message format
3. run() method - add your logic where marked "EDIT HERE"

Usage:
    pip install eclipse-zenoh pyyaml
    python main.py -e tcp/localhost:7447
"""

import argparse
import json
import logging
import signal
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict

import yaml
import zenoh

logging.basicConfig(level=logging.INFO, format="%(asctime)s - %(levelname)s - %(message)s")
logger = logging.getLogger("my-plugin")


# ============================================================================
# STEP 1: Define your configuration
# ============================================================================

@dataclass
class Config:
    topic: str = "my-plugin/data"
    interval_secs: int = 10
    # ADD YOUR CONFIG FIELDS HERE:
    # sensor_id: str = ""
    # threshold: float = 0.0

    @classmethod
    def from_dict(cls, d: Dict[str, Any]) -> "Config":
        return cls(
            topic=d.get("topic", cls.topic),
            interval_secs=d.get("interval_secs", cls.interval_secs),
        )


# ============================================================================
# STEP 2 & 3: Implement your plugin
# ============================================================================

class MyPluginNode:
    def __init__(self, session: zenoh.Session, config_dict: Dict[str, Any]):
        self.session = session
        self.config = Config.from_dict(config_dict)
        logger.info(f"Starting my-plugin, topic: {self.config.topic}")

    def run(self, shutdown: threading.Event) -> None:
        publisher = self.session.declare_publisher(self.config.topic)
        counter = 0

        while not shutdown.is_set():
            # ================================================
            # EDIT HERE: Your plugin logic
            # ================================================
            data = {
                "value": 42.0 + (counter * 0.1),
                "timestamp": int(time.time()),
                # ADD YOUR FIELDS HERE:
                # "temperature": read_sensor(),
            }

            publisher.put(json.dumps(data))
            logger.info(f"Published: {data}")
            counter += 1

            shutdown.wait(timeout=self.config.interval_secs)


# ============================================================================
# MAIN - No changes needed
# ============================================================================

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("-c", "--config", default="config.yaml")
    parser.add_argument("-e", "--endpoint", default="tcp/localhost:7447")
    args = parser.parse_args()

    # Load config
    config = {}
    if Path(args.config).exists():
        with open(args.config) as f:
            config = yaml.safe_load(f) or {}

    # Setup Zenoh
    zenoh_config = zenoh.Config()
    zenoh_config.insert_json5("connect/endpoints", json.dumps([args.endpoint]))

    # Shutdown handler
    shutdown = threading.Event()
    signal.signal(signal.SIGINT, lambda *_: shutdown.set())
    signal.signal(signal.SIGTERM, lambda *_: shutdown.set())

    logger.info(f"Connecting to {args.endpoint}")
    with zenoh.open(zenoh_config) as session:
        MyPluginNode(session, config).run(shutdown)


if __name__ == "__main__":
    main()
